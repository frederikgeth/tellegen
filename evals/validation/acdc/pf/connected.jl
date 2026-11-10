# Connected PF with explicit references/controls and physical SI substitutions.
include("../reference-model.jl")
include("../physical/reference-physical.jl")
include("../physical/reference-numerics.jl")
include("shared-voltage.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS));native=JSON.parsefile(joinpath(out,"native.json"))
@assert length(native)==384
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
options=merge(OPTIONS,Dict("constr_viol_tol"=>1e-10,"acceptable_iter"=>0,"mu_strategy"=>"adaptive","nlp_scaling_method"=>"none"))
records=[];models=[]
for case in native
    path=joinpath(out,case["file"]);data=PM.parse_file(path)
    declared=Dict(id=>s["Imax"] for (id,s) in data["convdc"])
    PowerModelsACDC.process_additional_data!(data)
    for (id,value) in declared;data["convdc"][id]["Imax"]=value;end
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
    map,vars,constants=mapping(pm,data);fs=factors(data,map)
    active=deepcopy(data)
    filter!(p->p.second["status"]==1,active["convdc"]);filter!(p->p.second["status"]==1,active["branchdc"])
    replacements=physical_equations!(pm,active,vars)
    scales=normalize_physical_rows!(pm,active,vars)
    sharing=shared_voltage_rows!(pm,data,vars);@assert length(sharing)==1
    for (id,s) in active["convdc"]
        s["type_dc"]==3 || continue
        p,v=vars["0_pconv_dc[$id]"],vars["0_vdcm[$(s["busdc_i"])]"]
        row=only([r for r in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64}) if Set(x for (a,x) in linear_terms(constraint_object(r).func) if a!=0)==Set([p,v])]);delete(pm.model,row)
        base=data["busdc"][string(s["busdc_i"])]["basekVdc"];mva=data["baseMVA"]
        slope=s["droop"]*base/mva;reference=s["Vdcset"]*base;target=s["Pdcset"]*mva
        @assert isapprox(slope,0.2;atol=1e-14)&&isapprox(reference,500.;atol=1e-10)
        @constraint(pm.model,mva*p==target-(base*v-reference)/slope)
        push!(replacements,"physical_dc_droop/$id")
    end
    @assert sort!([parse(Int,id) for (id,b) in data["bus"] if b["bus_type"]==3])==[r["bus"] for r in case["angle_references"]]
    solves=[];native_checks=[]
    for input in case["solves"]
        push!(native_checks,feasible(pm,mapped(input["point"],map,constants),scales))
        initial=mapped(input["initial_point"],map,constants)
        for (v,x) in initial;set_start_value(v,x);end
        duplicates=deduplicate_voltage_rows!(pm)
        fixed=copy(constants)
        for row in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
            obj=constraint_object(row);terms=[(a,v) for (a,v) in linear_terms(obj.func) if a!=0]
            length(terms)==1 || continue
            a,v=only(terms);target=(obj.set.value-constant(obj.func))/a
            @assert !haskey(fixed,v)||abs(fixed[v]-target)<1e-12
            fixed[v]=target
        end
        saved=auxiliary_presolve!(pm,fixed)
        set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...));optimize!(pm.model)
        status=termination_status(pm.model)
        @assert status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED) "$(case["group"]) base $(case["base_index"]): $status"
        solved=Dict(v=>value(v) for v in all_variables(pm.model))
        restore_auxiliaries!(pm,saved);restore_voltage_rows!(pm,duplicates)
        violation=feasible(pm,solved,scales)
        point=Dict(k=>solved[v] for (k,v) in map);physical=Dict(k=>point[k]*fs[k] for k in keys(map))
        delta=maximum(abs(physical[k]-input["physical"][k]) for k in keys(map));@assert delta<1e-6 "physical difference $delta"
        negative=0
        for key in ("dc_bus/dc_equivalent_bus/23/vm","station/ac_dc_converter_station/31/pdc","qg[mesh-generator-2]")
            bad=copy(solved);bad[map[key]]+=1/fs[key]
            @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.)>1e-4;negative+=1
        end
        push!(solves,Dict("start"=>input["start"],"point"=>point,"physical"=>physical,"termination"=>string(status),"max_reference_violation"=>violation,"max_native_physical_difference"=>delta,"negative_controls"=>negative))
    end
    push!(records,merge(Dict(k=>case[k] for k in ("file","case","group","bases","base_index","fingerprint")),Dict("solves"=>solves,"native_reference_violations"=>native_checks,"reference_replacements"=>replacements,"source_sha256"=>bytes2hex(sha256(read(path))))))
    push!(models,(pm,map,constants,fs,scales))
end
cross_checks=0;worst_cross=0.
for (i,target) in enumerate(native)
    pm,map,constants,fs,scales=models[i]
    for (j,source) in enumerate(native)
        source["group"]==target["group"]||continue
        for origin in (source["solves"],records[j]["solves"]),solve in origin
            point=Dict(k=>v/fs[k] for (k,v) in solve["physical"])
            global worst_cross=max(worst_cross,feasible(pm,mapped(point,map,constants),scales));global cross_checks+=1
        end
    end
end
@assert cross_checks==6144
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,Dict("records"=>records,"cross_checks"=>cross_checks,"max_cross_violation"=>worst_cross),2);end
println("768 reference connected PF solves / $cross_checks equivalent-base checks")
