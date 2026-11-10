# Independent PF solves: source equations plus explicitly qualified generator allocation.
include("../reference-model.jl")
include("mapping.jl")
include("../physical/reference-numerics.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
native=JSON.parsefile(joinpath(out,"native.json"))
@assert length(native)==8
cases=[]
options=merge(OPTIONS,Dict("constr_viol_tol"=>1e-10,"acceptable_iter"=>0,"mu_strategy"=>"adaptive","nlp_scaling_method"=>"none"))
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
    @assert length(sharing)==(case["case"]==7 ? 3 : 1)
    set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...))
    solves=[]
    for solve in case["solves"]
        println("PF $(case["name"]) / $(solve["start"])");flush(stdout)
        input=solve["result"]
        x,extra=native_point(input,pm,data,ids,vars)
        native_violation=feasible(pm,x,Dict())
        initial,_=native_point(Dict("point"=>input["initial_point"]),pm,data,ids,vars)
        for (v,value) in initial;set_start_value(v,value);end
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
        if !(status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED))
            println("FAILED ",case["name"],"/",solve["start"]," ",status)
            println(sort!(collect(primal_feasibility_report(pm.model));by=last,rev=true)[1:min(10,end)])
            println(Dict(name(v)=>value(v) for v in all_variables(pm.model) if occursin(r"vm|iconv|qconv|pconv",name(v))))
        end
        @assert status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED) "$status: $(case["name"])"
        reference=Dict(v=>value(v) for v in all_variables(pm.model))
        restore_auxiliaries!(pm,saved)
        @assert all(reference[v]>1e-8 for v in voltage_vars)
        for (v,bound) in voltage_bounds
            if bound===nothing;delete_lower_bound(v);else;set_lower_bound(v,bound);end
        end
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
        bad=copy(reference);bad[vars["0_qg[1]"]]+=0.01;bad[vars["0_qg[2]"]]-=0.01
        violations=primal_feasibility_report(pm.model,bad)
        @assert maximum(get(violations,c,0.0) for c in sharing)>0.01
        for key in ("0_pconv_tf_fr[1]","0_qconv_tf_fr[1]","0_vdcm[2]")
            bad=copy(reference);bad[vars[key]]+=0.01
            @assert maximum(values(primal_feasibility_report(pm.model,bad));init=0.)>1e-3
        end
        push!(solves,Dict("start"=>solve["start"],"point"=>point,"termination"=>string(status),"native_in_reference_violation"=>native_violation,"reference_violation"=>reference_violation,"max_electrical_difference"=>difference,"max_raw_coordinate_difference"=>maximum(abs(v) for v in values(delta)),"phase_windings_relative_to_native"=>phase_windings,"reference_only_variables"=>extra))
    end
    push!(cases,Dict("case"=>case["case"],"name"=>case["name"],"source_sha256"=>bytes2hex(sha256(read(path))),"sharing_equalities"=>length(sharing),"solves"=>solves))
end
result=Dict("cases"=>cases,"solver_options"=>options,"reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7","powerio_revision"=>only(unique(c["preparation"]["powerio_revision"] for c in native)),"julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"..","Manifest.toml")))),"native_report_sha256"=>bytes2hex(sha256(read(joinpath(out,"native.json")))),"reference_modification"=>"equal_increments_from_declared_generator_Pg_at_reference_and_Qg_at_PV_reference; positive_voltage_numerical_domain_with_original_bounds_restored; exact_presolve_of_affine_setpoints_fixed_load_and_unused_VSC_auxiliaries_restored_before_validation")
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,result,2);println(io);end
println("8 cases / 24 independent PF solves / 96 corruption controls pass")
