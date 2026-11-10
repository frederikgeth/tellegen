# Independent PF solves: source equations plus explicitly qualified generator allocation.
include("../reference-model.jl")
include("mapping.jl")
include("shared-voltage.jl")
include("case39-probes.jl")
include("../physical/reference-numerics.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
native=JSON.parsefile(joinpath(out,"native.json"))
suite=get(ENV,"TELLEGEN_ACDC_PF_SUITE","case5")
@assert suite in ("case5","case24","shared","case39","case67")
@assert length(native)==(suite=="case24" ? 11 : 8)
cases=[]
options=merge(OPTIONS,Dict("constr_viol_tol"=>1e-10,"acceptable_iter"=>0,"mu_strategy"=>"adaptive","nlp_scaling_method"=>"none"))
control_probes=suite=="case39" ? case39_control_probes(out,first(native)["ids"],options) : []
for case in native
    path=joinpath(out,case["file"]);ids=case["ids"]
    data=PM.parse_file(path)
    PowerModelsACDC.process_additional_data!(data)
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
    vars=Dict(name(v)=>v for v in all_variables(pm.model))
    # Allocate otherwise free outputs using source declarations, independently
    # of the native point. Voltage and nodal injections remain unchanged.
    sharing=[]
    for (id,bus) in data["bus"]
        gens=sort!([g for (g,s) in data["gen"] if s["gen_status"]==1 && s["gen_bus"]==parse(Int,id)];by=x->parse(Int,x))
        isempty(gens) && continue
        for g in gens[2:end], field in ("qg","pg")
            field=="pg" && bus["bus_type"]!=3 && continue
            field=="qg" && !(bus["bus_type"] in (2,3)) && continue
            first=gens[1];a,b=vars["0_$field[$g]"],vars["0_$field[$first]"]
            c=@constraint(pm.model,a-b==data["gen"][g][field]-data["gen"][first][field])
            push!(sharing,c)
        end
    end
    voltage_sharing=occursin("shared_voltage",case["preparation"]["profile"]) ? shared_voltage_rows!(pm,data,vars) : []
    append!(sharing,voltage_sharing)
    @assert length(sharing)==count(n->startswith(n,"pf/share_"),case["preparation"]["controls"])
    set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...))
    original=nothing
    if suite in ("case24","shared") && case["case"]==0
        original_data=source_case()
        PowerModelsACDC.process_additional_data!(original_data)
        original_pm=PM.instantiate_model(original_data,PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
        original=(original_pm,original_data,Dict(name(v)=>v for v in all_variables(original_pm.model)))
    end
    solves=[]
    for solve in case["solves"]
        println("PF $(case["name"]) / $(solve["start"])");flush(stdout)
        input=solve["result"]
        x,extra=native_point(input,pm,data,ids,vars)
        native_violation=feasible(pm,x,Dict())
        original_violation=nothing
        if original!==nothing
            opm,odata,ovars=original
            ox,_=native_point(input,opm,odata,ids,ovars)
            original_violation=feasible(opm,ox,Dict())
        end
        initial,_=native_point(Dict("point"=>input["initial_point"]),pm,data,ids,vars)
        for (v,value) in initial;set_start_value(v,value);end
        duplicates=deduplicate_voltage_rows!(pm)
        constants=Dict(vars[k]=>Float64(v["value"]) for (k,v) in extra)
        # Match the native positive-magnitude numerical domain, not voltage ratings.
        # Restore original reference bounds before accepting the resulting point.
        voltage_vars=[v for (n,v) in vars if occursin(r"^0_(vm|vmf|vmc|vdcm)\[",n)]
        voltage_bounds=Dict(v=>(has_lower_bound(v) ? lower_bound(v) : nothing) for v in voltage_vars)
        for v in voltage_vars;set_lower_bound(v,1e-8);end
        # Eliminate explicit affine setpoints exactly; otherwise Ipopt can drift
        # from fixed voltage targets while exploring an unrelated low-voltage root.
        for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
            obj=constraint_object(c)
            terms=[(coef,v) for (coef,v) in linear_terms(obj.func) if coef!=0]
            length(terms)==1 || continue
            coef,v=only(terms)
            target=(obj.set.value-constant(obj.func))/coef
            @assert !haskey(constants,v) || abs(constants[v]-target)<1e-12
            constants[v]=target
        end
        saved=auxiliary_presolve!(pm,constants)
        optimize!(pm.model)
        status=termination_status(pm.model)
        reference=Dict(v=>value(v) for v in all_variables(pm.model))
        restore_auxiliaries!(pm,saved)
        restore_voltage_rows!(pm,duplicates)
        for (v,bound) in voltage_bounds
            if bound===nothing;delete_lower_bound(v);else;set_lower_bound(v,bound);end
        end
        if !(status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED))
            @assert suite in ("case39","case67") "$status: $(case["name"])"
            violation=maximum(values(primal_feasibility_report(pm.model,reference));init=0.)
            println("Failed local reference trial $(case["name"])/$(solve["start"]): $status, residual $violation");flush(stdout)
            push!(solves,Dict("start"=>solve["start"],"termination"=>string(status),"native_in_reference_violation"=>native_violation,"reference_violation"=>violation,"failed_reference_iterate"=>Dict(name(v)=>x for (v,x) in reference)))
            continue
        end
        @assert all(reference[v]>1e-8 for v in voltage_vars)
        reference_violation=feasible(pm,reference,Dict())
        # Obtain inverse mapping by marker values; the adapter must be bijective.
        keys_native=sort!(collect(keys(input["point"])))
        markers=Dict(k=>Float64(i+1000) for (i,k) in enumerate(keys_native))
        mapped,_=native_point(Dict("point"=>markers),pm,data,ids,vars)
        reverse=Dict(Int(v)-1000=>r for (r,v) in mapped if !(name(r) in keys(extra)))
        @assert length(reverse)==length(keys_native)
        point=Dict(k=>reference[reverse[i]] for (i,k) in enumerate(keys_native))
        angular(k)=startswith(k,"va[") || endswith(k,"/vafilt") || endswith(k,"/vaconv")
        delta=Dict(k=>point[k]-input["point"][k] for k in keys_native)
        # Polar phases are periodic; retain raw points and report every winding.
        phase_windings=Dict(k=>round(Int,v/(2*pi)) for (k,v) in delta if angular(k) && abs(v)>pi)
        difference=maximum(angular(k) ? abs(atan(sin(v),cos(v))) : abs(v) for (k,v) in delta)
        @assert difference<1e-7 "electrical difference $difference"
        # The native result already satisfies the unmodified reference PF too;
        # sharing constrains only its otherwise undetermined per-generator split.
        qrows=[c for c in sharing if all(startswith(name(v),"0_qg[") for (coef,v) in linear_terms(constraint_object(c).func))]
        if !isempty(qrows)
            pair=[v for (coef,v) in linear_terms(constraint_object(first(qrows)).func)]
            @assert length(pair)==2
            bad=copy(reference);bad[pair[1]]+=0.01;bad[pair[2]]-=0.01
            violations=primal_feasibility_report(pm.model,bad)
            @assert maximum(get(violations,c,0.0) for c in sharing)>0.01
        else
            # Single generators need no sharing row: corrupt the declared AC angle reference.
            refid=first(id for (id,b) in data["bus"] if b["bus_type"]==3)
            bad=copy(reference);bad[vars["0_va[$refid]"]]+=0.01
            @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.)>0.009
        end
        active=sort!([id for (id,s) in data["convdc"] if s["status"]==1];by=x->parse(Int,x))
        p_id=first(id for id in active if data["convdc"][id]["type_dc"]==1)
        q_id=first(id for id in active if data["convdc"][id]["type_ac"]==1)
        dc_id=data["convdc"][first(id for id in active if data["convdc"][id]["type_dc"] in (2,3))]["busdc_i"]
        for key in ("0_pconv_tf_fr[$p_id]","0_qconv_tf_fr[$q_id]","0_vdcm[$dc_id]")
            bad=copy(reference);bad[vars[key]]+=0.01
            @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.)>1e-3
        end
        droop_corruptions=0
        for id in active
            c=data["convdc"][id]
            c["type_dc"]==3 || continue
            pdc=vars["0_pconv_dc[$id]"];vdc=vars["0_vdcm[$(c["busdc_i"])]"]
            rows=[r for r in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64}) if Set(v for (a,v) in linear_terms(constraint_object(r).func) if a!=0)==Set([pdc,vdc])]
            row=only(rows);obj=constraint_object(row)
            @assert coefficient(obj.func,pdc)==1 && coefficient(obj.func,vdc)==1/c["droop"]
            for (v,delta) in ((pdc,0.01),(vdc,0.001))
                bad=copy(reference);bad[v]+=delta
                @assert primal_feasibility_report(pm.model,bad)[row]>0.009
                droop_corruptions+=1
            end
        end
        for row in voltage_sharing
            terms=collect(linear_terms(constraint_object(row).func))
            @assert length(terms)==2
            (a,v),(b,w)=terms
            bad=copy(reference);bad[v]+=0.01;bad[w]+=sign(a*b)*0.01
            # Same-sign coefficients mean mixed injection/withdrawal variables;
            # opposite signs mean two withdrawals. Either perturbation keeps net Q.
            @assert primal_feasibility_report(pm.model,bad)[row]>0.019
        end
        push!(solves,Dict("corruption_controls"=>4+length(voltage_sharing)+droop_corruptions,"duplicate_voltage_rows_restored"=>length(duplicates),"start"=>solve["start"],"point"=>point,"termination"=>string(status),"native_in_reference_violation"=>native_violation,"native_in_unmodified_source_violation"=>original_violation,"reference_violation"=>reference_violation,"max_electrical_difference"=>difference,"max_raw_coordinate_difference"=>maximum(abs(v) for v in values(delta)),"phase_windings_relative_to_native"=>phase_windings,"reference_only_variables"=>extra))
    end
    @assert count(s->haskey(s,"point"),solves)>=(suite in ("case39","case67") ? 2 : 3) "insufficient independent reference solves"
    push!(cases,Dict("case"=>case["case"],"name"=>case["name"],"source_sha256"=>bytes2hex(sha256(read(path))),"sharing_equalities"=>length(sharing),"solves"=>solves))
end
result=Dict("control_only_probes"=>control_probes,"suite"=>suite,"cases"=>cases,"solver_options"=>options,"reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7","powerio_revision"=>only(unique(c["preparation"]["powerio_revision"] for c in native)),"julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"..","Manifest.toml")))),"native_report_sha256"=>bytes2hex(sha256(read(joinpath(out,"native.json")))),"reference_modification"=>"equal_increments_from_declared_generator_Pg_at_reference_and_Qg_at_PV_reference; shared_voltage_injection_increments_only_when_explicit_profile; duplicate_Vm_rows_restored; positive_voltage_numerical_domain_with_original_bounds_restored; exact_presolve_of_affine_setpoints_fixed_load_and_unused_VSC_auxiliaries_restored_before_validation")
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,result,2);println(io);end
println("$(length(cases)) cases / $(sum(haskey(s,"point") for c in cases for s in c["solves"])) independent PF solves / $(sum(get(s,"corruption_controls",0) for c in cases for s in c["solves"])) corruption controls pass")
