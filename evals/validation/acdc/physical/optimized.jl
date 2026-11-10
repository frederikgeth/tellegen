# Independent local solves and bidirectional equivalent-base cross-feasibility.
include("../reference-model.jl")
include("reference-physical.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
native=JSON.parsefile(joinpath(out,"native.json"))
@assert length(native)==128
const PREFIX="station/ac_dc_converter_station/31/"
function mapping(pm)
    vars=Dict(name(v)=>v for v in all_variables(pm.model))
    pairs=Dict("vm[11]"=>"0_vm[11]","va[11]"=>"0_va[11]",
        "pg[physical-generator]"=>"0_pg[1]","qg[physical-generator]"=>"0_qg[1]",
        "dc_bus/dc_equivalent_bus/21/vm"=>"0_vdcm[21]","dc_bus/dc_equivalent_bus/22/vm"=>"0_vdcm[22]",
        "dc_branch/dc_equivalent_branch/41/pf"=>"0_pdcgrid[(1, 21, 22)]",
        "dc_branch/dc_equivalent_branch/41/pt"=>"0_pdcgrid[(1, 22, 21)]")
    for (n,r) in [("pgrid","pconv_tf_fr"),("qgrid","qconv_tf_fr"),("ptf_to","pconv_tf_to"),("qtf_to","qconv_tf_to"),
        ("ppr_fr","pconv_pr_fr"),("qpr_fr","qconv_pr_fr"),("pconv","pconv_ac"),("qconv","qconv_ac"),
        ("pdc","pconv_dc"),("iconv","iconv_ac"),("vmfilt","vmf"),("vafilt","vaf"),("vmconv","vmc"),("vaconv","vac")]
        pairs[PREFIX*n]="0_"*r*"[1]"
    end
    @assert Set(values(pairs))==setdiff(Set(keys(vars)),Set(["0_phiconv[1]"]))
    return Dict(k=>vars[v] for (k,v) in pairs),vars
end
function mapped(point,map,vars)
    @assert Set(keys(point))==Set(keys(map))
    x=Dict(map[k]=>Float64(v) for (k,v) in point)
    x[vars["0_phiconv[1]"]]=0.0
    return x
end
# Derive voltage/current factors from parsed declarations, not native ratios.
function factors(data,map)
    s=data["convdc"]["1"]
    f=Dict{String,Float64}()
    for k in keys(map)
        f[k]=if k=="vm[11]"
            data["bus"]["11"]["base_kv"]
        elseif startswith(k,"dc_bus/")
            data["busdc"][split(k,"/")[3]]["basekVdc"]
        elseif endswith(k,"/vmfilt") || endswith(k,"/vmconv")
            s["basekVac"]
        elseif endswith(k,"/iconv")
            data["baseMVA"]/(sqrt(3)*s["basekVac"])
        elseif k=="va[11]" || endswith(k,"/vafilt") || endswith(k,"/vaconv")
            1.0
        else
            data["baseMVA"]
        end
    end
    return f
end
function feasible(pm,x)
    violations=primal_feasibility_report(pm.model,x)
    worst=maximum(values(violations);init=0.0)
    @assert isfinite(worst) && worst<1e-7 "reference violation $worst: $violations"
    return worst
end
function costcheck(a,b)
    @assert abs(a-b)<1e-6+1e-8*max(abs(a),abs(b)) "cost $a != $b"
end
records=[]
models=[]
for case in native
    path=joinpath(out,case["file"])
    data=PM.parse_file(path)
    declared=data["convdc"]["1"]["Imax"]
    PowerModelsACDC.process_additional_data!(data)
    data["convdc"]["1"]["Imax"]=declared
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcopf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
    map,vars=mapping(pm)
    replacements=physical_equations!(pm,data,vars)
    @assert length(replacements)==((case["mask"] & 1)==0 ? 3 : 2)
    fs=factors(data,map)
    for (key,v) in map
        expected=case["physical_bounds"][key]
        for (present,bound,index) in [(has_lower_bound(v),lower_bound,1),(has_upper_bound(v),upper_bound,2)]
            @assert present == (expected[index] !== nothing) "bound presence $key"
            if present; @assert isapprox(bound(v)*fs[key],expected[index];atol=1e-10,rtol=1e-12) "physical bound $key";end
        end
    end
    set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,OPTIONS...))
    solves=[]
    native_checks=[]
    for input in case["solves"]
        x=mapped(input["point"],map,vars)
        push!(native_checks,feasible(pm,x))
        costcheck(value(v->x[v],objective_function(pm.model)),input["objective"])
        for (k,v) in input["physical"]
            @assert isapprox(input["point"][k]*fs[k],v;atol=1e-10,rtol=1e-12)
        end
        initial=mapped(input["initial_point"],map,vars)
        for (v,x0) in initial;set_start_value(v,x0);end
        optimize!(pm.model)
        status=termination_status(pm.model)
        @assert status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED) "$status: $(case["file"])"
        solved=Dict(v=>value(v) for v in all_variables(pm.model))
        violation=feasible(pm,solved)
        objective=objective_value(pm.model)
        costcheck(objective,input["objective"])
        point=Dict(k=>solved[v] for (k,v) in map)
        physical=Dict(k=>point[k]*fs[k] for k in keys(map))
        pg=physical["pg[physical-generator]"]
        costcheck(objective,0.002*pg^2+3*pg+1)
        # Independent physical negative controls, in kV and MW.
        bad=copy(solved);bad[map["dc_bus/dc_equivalent_bus/22/vm"]]+=1/fs["dc_bus/dc_equivalent_bus/22/vm"]
        @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.0)>1e-3
        bad=copy(solved);bad[map[PREFIX*"pgrid"]]+=1/data["baseMVA"]
        @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.0)>1e-3
        push!(solves,Dict("start"=>input["start"],"point"=>point,"physical"=>physical,"objective"=>objective,
            "max_reference_violation"=>violation,"termination"=>string(status),"native_objective_difference"=>objective-input["objective"]))
    end
    push!(records,merge(Dict(k=>case[k] for k in ("file","group","mask","direction","poles","bases","base_index","fingerprint")),
        Dict("source_sha256"=>bytes2hex(sha256(read(path))),"solves"=>solves,"native_reference_violations"=>native_checks,"reference_replacements"=>replacements)))
    push!(models,(pm,map,vars,fs))
end
# Both solvers' physical solutions must satisfy every differently based model.
cross_checks=0
worst_cross=0.0
for (i,target) in enumerate(native)
    pm,map,vars,fs=models[i]
    for (j,source) in enumerate(native)
        source["group"]==target["group"] || continue
        for origin in (source["solves"],records[j]["solves"]), solve in origin
            point=Dict(k=>v/fs[k] for (k,v) in solve["physical"])
            x=mapped(point,map,vars)
            global worst_cross=max(worst_cross,feasible(pm,x))
            costcheck(value(v->x[v],objective_function(pm.model)),solve["objective"])
            costcheck(solve["objective"],records[i]["solves"][1]["objective"])
            global cross_checks+=1
        end
    end
end
@assert cross_checks==2048
output=Dict("records"=>records,"reference_modification"=>"restore_source_Imax; SI_transformer_bypass_and_DC_Ohms_with_from_side_resistance_base",
    "reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7","powerio_revision"=>only(unique(c["powerio_revision"] for c in native)),
    "julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"..","Manifest.toml")))),
    "native_report_sha256"=>bytes2hex(sha256(read(joinpath(out,"native.json")))),"cross_checks"=>cross_checks,"max_cross_violation"=>worst_cross)
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,output,2);println(io);end
println("128 networks / 256 reference solves; $cross_checks equivalent-base checks; worst $worst_cross")
