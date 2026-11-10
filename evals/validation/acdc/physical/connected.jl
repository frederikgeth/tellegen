# Independent local solves and bidirectional equivalent-base cross-feasibility.
include("../reference-model.jl")
include("reference-physical.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
native=JSON.parsefile(joinpath(out,"native.json"))
@assert length(native)==128
function mapping(pm,data)
    vars=Dict(name(v)=>v for v in all_variables(pm.model))
    pairs=Dict{String,String}()
    fields=[("pgrid","pconv_tf_fr"),("qgrid","qconv_tf_fr"),("ptf_to","pconv_tf_to"),("qtf_to","qconv_tf_to"),
        ("ppr_fr","pconv_pr_fr"),("qpr_fr","qconv_pr_fr"),("pconv","pconv_ac"),("qconv","qconv_ac"),
        ("pdc","pconv_dc"),("iconv","iconv_ac"),("vmfilt","vmf"),("vafilt","vaf"),("vmconv","vmc"),("vaconv","vac")]
    for i in 1:3
        ac,dc,station,line=10+i,20+i,30+i,40+i
        to=21+i%3
        @assert data["gen"][string(i)]["gen_bus"]==ac
        @assert data["convdc"][string(i)]["busac_i"]==ac && data["convdc"][string(i)]["busdc_i"]==dc
        @assert (data["branchdc"][string(i)]["fbusdc"],data["branchdc"][string(i)]["tbusdc"])==(dc,to)
        for f in ("vm","va");pairs["$f[$ac]"]="0_$f[$ac]";end
        for f in ("pg","qg");pairs["$f[mesh-generator-$i]"]="0_$f[$i]";end
        pairs["dc_bus/dc_equivalent_bus/$dc/vm"]="0_vdcm[$dc]"
        if data["branchdc"][string(i)]["status"]==1
            pairs["dc_branch/dc_equivalent_branch/$line/pf"]="0_pdcgrid[($i, $dc, $to)]"
            pairs["dc_branch/dc_equivalent_branch/$line/pt"]="0_pdcgrid[($i, $to, $dc)]"
        end
        if data["convdc"][string(i)]["status"]==1
            for (n,r) in fields;pairs["station/ac_dc_converter_station/$station/$n"]="0_"*r*"[$i]";end
        end
        lineac=data["branch"][string(i)]
        @assert (lineac["f_bus"],lineac["t_bus"])==(ac,11+i%3)
        if lineac["br_status"]==1
            for (suffix,f,t) in [("f",lineac["f_bus"],lineac["t_bus"]),("t",lineac["t_bus"],lineac["f_bus"])], field in ("p","q")
                pairs["ac_branch/physical-ac-$i/$field$suffix"]="0_$field[($i, $f, $t)]"
            end
        end
    end
    constants=Dict{VariableRef,Float64}()
    for (id,load) in data["load"]
        @assert load["flex"]==0
        for (field,v) in [("pflex",load["pd"]),("qflex",load["qd"]),("pcurt",0.0)];constants[vars["0_$field[$id]"]]=v;end
    end
    for (id,s) in data["convdc"];if s["status"]==1;constants[vars["0_phiconv[$id]"]]=0.0;end;end
    @assert length(pairs)+length(constants)==length(vars)
    @assert Set(values(pairs))==setdiff(Set(keys(vars)),Set(name(v) for v in keys(constants)))
    return Dict(k=>vars[v] for (k,v) in pairs),vars,constants
end
function mapped(point,map,constants)
    @assert Set(keys(point))==Set(keys(map))
    return merge(Dict(map[k]=>Float64(v) for (k,v) in point),constants)
end
function factors(data,map)
    f=Dict{String,Float64}()
    for k in keys(map)
        f[k]=if startswith(k,"vm[")
            data["bus"][match(r"vm\[(\d+)\]",k).captures[1]]["base_kv"]
        elseif startswith(k,"va[")
            1.0
        elseif startswith(k,"dc_bus/")
            data["busdc"][split(k,"/")[3]]["basekVdc"]
        elseif startswith(k,"station/")
            id=string(parse(Int,split(k,"/")[3])-30);field=split(k,"/")[4];s=data["convdc"][id]
            field in ("vmfilt","vmconv") ? s["basekVac"] : field in ("vafilt","vaconv") ? 1.0 : field=="iconv" ? data["baseMVA"]/(sqrt(3)*s["basekVac"]) : data["baseMVA"]
        else
            data["baseMVA"]
        end
    end
    return f
end
include("reference-numerics.jl")
# Preserve the declared idle-current initialization near its lower bound.
# Ipopt's default 0.01 interior push displaced it; these are initialization
# settings, not a relaxation of bounds or acceptance tolerances.
const CONNECTED_OPTIONS=merge(OPTIONS,Dict("constr_viol_tol"=>1e-9,"acceptable_constr_viol_tol"=>1e-9,"acceptable_iter"=>0,"mu_strategy"=>"monotone","bound_push"=>1e-8,"bound_frac"=>1e-8,"nlp_scaling_method"=>"none"))
records=[]
models=[]
for case in native
    println("Checking $(case["file"]): $(case["group"]), base $(case["base_index"])");flush(stdout)
    path=joinpath(out,case["file"])
    data=PM.parse_file(path)
    topology=case["case"]["topology"]
    for i in 1:3
        ac_on=topology==1 ? i!=1 : topology in (2,6,7) ? i==1 : topology!=3
        dc_on=topology==4 ? i!=1 : topology in (5,6) ? i==1 : true
        @assert data["branch"][string(i)]["br_status"]==Int(ac_on)
        @assert data["branchdc"][string(i)]["status"]==Int(dc_on)
        @assert data["convdc"][string(i)]["status"]==Int(topology!=7 || i!=3)
    end
    declared=Dict(id=>v["Imax"] for (id,v) in data["convdc"])
    PowerModelsACDC.process_additional_data!(data)
    for (id,v) in declared;data["convdc"][id]["Imax"]=v;end
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcopf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
    map,vars,constants=mapping(pm,data)
    @assert [data["bus"][string(i)]["bus_type"] for i in 11:13]==[3,2,2]
    anchors=[]
    for component in sort!(collect(PM.calc_connected_components(data;edges=["branch"]));by=minimum)
        declared=sort!([i for i in component if data["bus"][string(i)]["bus_type"]==3])
        refs=isempty(declared) ? [minimum(component)] : declared
        for bus in refs
            if isempty(declared);@constraint(pm.model,vars["0_va[$bus]"]==0);end
            push!(anchors,Dict("bus"=>bus,"island_buses"=>sort!(collect(component)),"declared"=>!isempty(declared)))
        end
    end
    sort!(anchors;by=r->r["bus"])
    @assert anchors==case["angle_references"]
    active=deepcopy(data)
    filter!(p->p.second["status"]==1,active["convdc"])
    filter!(p->p.second["status"]==1,active["branchdc"])
    replacements=physical_equations!(pm,active,vars)
    @assert length(replacements)==2*length(active["branchdc"])+count(s["transformer"]==0 for s in values(active["convdc"]))
    scales=normalize_physical_rows!(pm,active,vars)
    @assert length(scales)==length(replacements)
    fs=factors(data,map)
    for (key,v) in map
        expected=case["physical_bounds"][key]
        if startswith(key,"ac_branch/")
            # PowerModels adds redundant component bounds implied by its and
            # Tellegen's terminal apparent-power circles. Retain and verify them.
            @assert all(isnothing,expected)
            id=split(split(key,"/")[2],"-")[end]
            rating=data["branch"][id]["rate_a"]*data["baseMVA"]
            @assert isapprox(rating,45.0;atol=1e-12)
            expected=[-rating,rating]
        end
        for (present,bound,index) in [(has_lower_bound(v),lower_bound,1),(has_upper_bound(v),upper_bound,2)]
            @assert present == (expected[index] !== nothing) "bound presence $key"
            if present; @assert isapprox(bound(v)*fs[key],expected[index];atol=1e-10,rtol=1e-12) "physical bound $key";end
        end
    end
    set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,CONNECTED_OPTIONS...))
    solves=[]
    native_checks=[]
    for input in case["solves"]
        x=mapped(input["point"],map,constants)
        push!(native_checks,feasible(pm,x,scales))
        costcheck(value(v->x[v],objective_function(pm.model)),input["objective"])
        for (k,v) in input["physical"]
            @assert isapprox(input["point"][k]*fs[k],v;atol=1e-10,rtol=1e-12)
        end
        initial=mapped(input["initial_point"],map,constants)
        for (v,x0) in initial;set_start_value(v,x0);end
        saved=auxiliary_presolve!(pm,constants)
        optimize!(pm.model)
        status=termination_status(pm.model)
        @assert status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED) "$status ($(raw_status(pm.model))): $(case["file"]) / $(input["start"])"
        solved=Dict(v=>value(v) for v in all_variables(pm.model))
        objective=objective_value(pm.model)
        restore_auxiliaries!(pm,saved)
        violation=feasible(pm,solved,scales)
        costcheck(objective,input["objective"])
        point=Dict(k=>solved[v] for (k,v) in map)
        physical=Dict(k=>point[k]*fs[k] for k in keys(map))
        direct_cost=0.0
        for i in 0:2
            pg=physical["pg[mesh-generator-$(i+1)]"]
            linear=i==case["case"]["cheap"] ? 1.0 : 4.0+i
            direct_cost+=0.002*pg^2+linear*pg+1
        end
        costcheck(objective,direct_cost)
        bad=copy(solved);bad[map["dc_bus/dc_equivalent_bus/22/vm"]]+=1/fs["dc_bus/dc_equivalent_bus/22/vm"]
        @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.0)>1e-3
        bad=copy(solved);bad[map["station/ac_dc_converter_station/31/pgrid"]]+=1/data["baseMVA"]
        @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.0)>1e-3
        # Rotating only a PCC angle breaks either its station or AC branch equations.
        bad=copy(solved);bad[map["va[11]"]]+=0.01
        @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.0)>1e-3
        push!(solves,Dict("start"=>input["start"],"point"=>point,"physical"=>physical,"objective"=>objective,
            "max_reference_violation"=>violation,"termination"=>string(status),"native_objective_difference"=>objective-input["objective"]))
    end
    push!(records,merge(Dict(k=>case[k] for k in ("file","group","case","bases","base_index","fingerprint","angle_references")),
        Dict("source_sha256"=>bytes2hex(sha256(read(path))),"solves"=>solves,"native_reference_violations"=>native_checks,"reference_replacements"=>replacements,"constant_auxiliaries_presolved_and_restored"=>Dict(name(v)=>x for (v,x) in constants))))
    push!(models,(pm,map,vars,constants,fs,scales))
end
# Both solvers' physical solutions must satisfy every differently based model.
cross_checks=0
worst_cross=0.0
for (i,target) in enumerate(native)
    pm,map,vars,constants,fs,scales=models[i]
    for (j,source) in enumerate(native)
        source["group"]==target["group"] || continue
        for origin in (source["solves"],records[j]["solves"]), solve in origin
            point=Dict(k=>v/fs[k] for (k,v) in solve["physical"])
            x=mapped(point,map,constants)
            global worst_cross=max(worst_cross,feasible(pm,x,scales))
            costcheck(value(v->x[v],objective_function(pm.model)),solve["objective"])
            costcheck(solve["objective"],records[i]["solves"][1]["objective"])
            global cross_checks+=1
        end
    end
end
@assert cross_checks==2048
output=Dict("records"=>records,"reference_modification"=>"restore_source_Imax; SI_transformer_bypass_and_DC_Ohms_with_from_side_resistance_base; normalized_SI_rows_for_optimization_with_original_unit_validation; exact_presolve_of_declared_constant_auxiliaries_restore_original_rows_and_bounds_for_validation; explicit_AC_component_angle_anchors; retain_redundant_AC_flow_component_bounds",
    "reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7","powerio_revision"=>only(unique(c["powerio_revision"] for c in native)),
    "julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"..","Manifest.toml")))),
    "solver_options"=>CONNECTED_OPTIONS,"native_report_sha256"=>bytes2hex(sha256(read(joinpath(out,"native.json")))),"cross_checks"=>cross_checks,"max_cross_violation"=>worst_cross)
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,output,2);println(io);end
println("128 networks / 256 reference solves; $cross_checks equivalent-base checks; worst $worst_cross")
