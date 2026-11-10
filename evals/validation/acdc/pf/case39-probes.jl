# Control-only trials retain the source generator dispatch. They are diagnostic
# trials, separate from the qualified variants that explicitly select OPF Pg.
function case39_control_probes(out,ids,options)
    native=JSON.parsefile(joinpath(out,"control-only-native.json"))
    results=[]
    for probe in native
        data=source_case()
        selected=string(probe["selected_converter"])
        @assert all(c["type_dc"]==1 for c in values(data["convdc"]))
        data["convdc"][selected]["type_dc"]=2
        PowerModelsACDC.process_additional_data!(data)
        pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
        vars=Dict(name(v)=>v for v in all_variables(pm.model))
        set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...))
        trials=[]
        for trial in probe["trials"]
            println("Control-only probe converter $selected / $(trial["start"])");flush(stdout)
            initial,extra=native_point(Dict("point"=>trial["initial_point"]),pm,data,ids,vars)
            for (v,value) in initial;set_start_value(v,value);end
            voltages=[v for (n,v) in vars if occursin(r"^0_(vm|vmf|vmc|vdcm)\[",n)]
            bounds=Dict(v=>(has_lower_bound(v) ? lower_bound(v) : nothing) for v in voltages)
            for v in voltages;set_lower_bound(v,1e-8);end
            constants=Dict(vars[k]=>Float64(v["value"]) for (k,v) in extra)
            for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
                obj=constraint_object(c);terms=[(coef,v) for (coef,v) in linear_terms(obj.func) if coef!=0]
                length(terms)==1 || continue
                coef,v=only(terms);target=(obj.set.value-constant(obj.func))/coef
                @assert !haskey(constants,v) || abs(constants[v]-target)<1e-12
                constants[v]=target
            end
            saved=auxiliary_presolve!(pm,constants)
            optimize!(pm.model)
            status=termination_status(pm.model)
            point=has_values(pm.model) ? Dict(v=>value(v) for v in all_variables(pm.model)) : nothing
            restore_auxiliaries!(pm,saved)
            for (v,bound) in bounds
                if bound===nothing;delete_lower_bound(v);else;set_lower_bound(v,bound);end
            end
            violation=point===nothing ? nothing : maximum(values(primal_feasibility_report(pm.model,point));init=0.)
            if trial["native"]["termination"]=="CONVERGED"
                nx,_=native_point(trial["native"],pm,data,ids,vars)
                feasible(pm,nx,Dict())
            end
            push!(trials,Dict("start"=>trial["start"],"native"=>trial["native"],"reference_termination"=>string(status),"reference_max_violation"=>violation,"reference_point"=>(point===nothing ? nothing : Dict(name(v)=>x for (v,x) in point))))
        end
        push!(results,Dict("selected_converter"=>probe["selected_converter"],"trials"=>trials))
    end
    open(joinpath(out,"control-only-reference.json"),"w") do io;JSON.print(io,results,2);println(io);end
    compact=deepcopy(results)
    for p in compact,t in p["trials"];pop!(t,"reference_point");end
    compact
end
