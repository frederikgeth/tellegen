# Physical-coordinate PF: PMACDC with explicit SI substitutions, not source equivalence.
include("../reference-model.jl")
include("../physical/reference-physical.jl")
include("../physical/reference-numerics.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
native=JSON.parsefile(joinpath(out,"native.json"))
@assert length(native)==288
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
records=[]
models=[]
options=merge(OPTIONS,Dict("constr_viol_tol"=>1e-10,"acceptable_iter"=>0,"mu_strategy"=>"adaptive","nlp_scaling_method"=>"none"))
for case in native
    path=joinpath(out,case["file"])
    data=PM.parse_file(path)
    declared=data["convdc"]["1"]["Imax"]
    PowerModelsACDC.process_additional_data!(data)
    data["convdc"]["1"]["Imax"]=declared
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
    map,vars=mapping(pm)
    replacements=physical_equations!(pm,data,vars)
    @assert length(replacements)==((case["mask"] & 1)==0 ? 3 : 2)
    fs=factors(data,map)
    if case["droop"]
        s=data["convdc"]["1"]
        p,v=vars["0_pconv_dc[1]"],vars["0_vdcm[21]"]
        row=only([r for r in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64}) if Set(x for (a,x) in linear_terms(constraint_object(r).func) if a!=0)==Set([p,v])])
        delete(pm.model,row)
        base=data["busdc"]["21"]["basekVdc"];mva=data["baseMVA"]
        resistance=s["droop"]*base/mva
        reference=s["Vdcset"]*base
        @assert isapprox(resistance,0.2;atol=1e-14)
        @assert isapprox(reference,498.;atol=1e-11)
        target=s["Pdcset"]*mva
        @constraint(pm.model,mva*p==target-(base*v-reference)/resistance)
        push!(replacements,"physical_dc_droop/1")
    end
    station=data["convdc"]["1"];mode=only(case["loss_modes"])["mode"]
    @assert only(case["loss_modes"])["component"]=="ac_dc_converter_station/31"
    pc,pdc,ic=vars["0_pconv_ac[1]"],vars["0_pconv_dc[1]"],vars["0_iconv_ac[1]"]
    row=only([c for c in all_constraints(pm.model,QuadExpr,MOI.EqualTo{Float64}) if coefficient(constraint_object(c).func,pc)!=0 && coefficient(constraint_object(c).func,pdc)!=0])
    @assert isapprox(coefficient(constraint_object(row).func,ic,ic),-station["LossCinv"];atol=1e-14)
    delete(pm.model,row)
    mva=data["baseMVA"];ibase=mva/(sqrt(3)*station["basekVac"])
    a=station["LossA"]*mva;b=station["LossB"]*mva/ibase
    c=station[mode=="rectifier" ? "LossCrec" : "LossCinv"]*mva/ibase^2
    @assert isapprox(c,mode=="rectifier" ? 0.3 : 1.3;atol=1e-12)
    @constraint(pm.model,mva*(pc+pdc)==a+b*(ibase*ic)+c*(ibase*ic)^2)
    set_mode_bound!() = mode=="rectifier" ? set_lower_bound(pc,0.) : set_upper_bound(pc,0.)
    clear_mode_bound!() = mode=="rectifier" ? delete_lower_bound(pc) : delete_upper_bound(pc)
    set_mode_bound!()
    push!(replacements,"physical_fixed_loss/"*mode)
    scales=normalize_physical_rows!(pm,data,vars)
    solves=[];native_checks=[]
    for input in case["solves"]
        x=mapped(input["point"],map,vars)
        push!(native_checks,feasible(pm,x,scales))
        initial=mapped(input["initial_point"],map,vars)
        for (v,x0) in initial;set_start_value(v,x0);end
        constants=Dict(vars["0_phiconv[1]"]=>0.)
        for row in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
            obj=constraint_object(row);terms=[(a,v) for (a,v) in linear_terms(obj.func) if a!=0]
            length(terms)==1 || continue
            a,v=only(terms);target=(obj.set.value-constant(obj.func))/a
            @assert !haskey(constants,v) || abs(constants[v]-target)<1e-12
            constants[v]=target
        end
        saved=auxiliary_presolve!(pm,constants)
        # Solve the smooth selected-curve extension, as native Newton does.
        # Restore mode membership before accepting or cross-checking the root.
        clear_mode_bound!()
        set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...))
        optimize!(pm.model)
        status=termination_status(pm.model)
        @assert status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED) "$(case["file"]) / $(input["start"]): $status"
        solved=Dict(v=>value(v) for v in all_variables(pm.model))
        restore_auxiliaries!(pm,saved)
        set_mode_bound!()
        violation=feasible(pm,solved,scales)
        point=Dict(k=>solved[v] for (k,v) in map)
        physical=Dict(k=>point[k]*fs[k] for k in keys(map))
        difference=maximum(abs(physical[k]-input["physical"][k]) for k in keys(map))
        @assert difference<1e-6 "physical difference $difference"
        for (key,delta) in [("dc_bus/dc_equivalent_bus/22/vm",1.),(PREFIX*"pgrid",1.),(PREFIX*"iconv",0.01)]
            bad=copy(solved);bad[map[key]]+=delta/fs[key]
            @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.)>1e-4
        end
        push!(solves,Dict("start"=>input["start"],"point"=>point,"physical"=>physical,"termination"=>string(status),"max_reference_violation"=>violation,"max_native_physical_difference"=>difference,"negative_controls"=>3))
    end
    push!(records,merge(Dict(k=>case[k] for k in ("file","group","case","mask","droop","bases","base_index","fingerprint","loss_modes")),Dict("source_sha256"=>bytes2hex(sha256(read(path))),"solves"=>solves,"native_reference_violations"=>native_checks,"reference_replacements"=>replacements)))
    push!(models,(pm,map,vars,fs,scales))
end
cross_checks=0;worst_cross=0.
for (i,target) in enumerate(native)
    pm,map,vars,fs,scales=models[i]
    for (j,source) in enumerate(native)
        source["group"]==target["group"] || continue
        for origin in (source["solves"],records[j]["solves"]),solve in origin
            point=Dict(k=>v/fs[k] for (k,v) in solve["physical"])
            global worst_cross=max(worst_cross,feasible(pm,mapped(point,map,vars),scales))
            global cross_checks+=1
        end
    end
end
@assert cross_checks==4608
open(joinpath(out,"reference.json"),"w") do io
    JSON.print(io,Dict("records"=>records,"cross_checks"=>cross_checks,"max_cross_violation"=>worst_cross),2)
end
println("576 independent fixed-mode PF solves; $cross_checks equivalent-base checks")
