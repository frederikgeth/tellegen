# Reference-only PF contract gate; this does not enable a native droop PF solver.
ENV["TELLEGEN_ACDC_CASE"]="case67"
include("../../reference-model.jl")
include("../mapping.jl")
include("../shared-voltage.jl")
include("../../physical/reference-numerics.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
inputs=JSON.parsefile(joinpath(out,"inputs.json"))
options=merge(OPTIONS,Dict("constr_viol_tol"=>1e-10,"acceptable_iter"=>0,"mu_strategy"=>"adaptive","nlp_scaling_method"=>"none"))
function islands(data)
    remaining=Set(parse(Int,k) for (k,b) in data["bus"] if b["bus_type"]!=4)
    groups=[]
    while !isempty(remaining)
        root=minimum(remaining);delete!(remaining,root);group=[root];todo=[root]
        while !isempty(todo)
            v=pop!(todo)
            for b in values(data["branch"])
                b["br_status"]==1 || continue
                other=b["f_bus"]==v ? b["t_bus"] : b["t_bus"]==v ? b["f_bus"] : nothing
                if other in remaining;delete!(remaining,other);push!(todo,other);push!(group,other);end
            end
        end
        push!(groups,Dict("buses"=>sort!(group),"references"=>sort!([i for i in group if data["bus"][string(i)]["bus_type"]==3])))
    end
    groups
end
original=source_case()
original_islands=islands(original)
@assert only(g["buses"] for g in original_islands if isempty(g["references"]))==[67]
@assert original["bus"]["67"]["bus_type"]==2
@assert count(s->s["type_dc"]==3,values(original["convdc"]))==7
original_effective=deepcopy(original);PowerModelsACDC.process_additional_data!(original_effective)
# Data preparation divides Pdcset by baseMVA, but does not rescale the pu/pu slope.
for (id,c) in original["convdc"]
    @assert original_effective["convdc"][id]["Pdcset"]==c["Pdcset"]/original["baseMVA"]
    @assert original_effective["convdc"][id]["droop"]==c["droop"]
end
cases=[]
for case in inputs
    path=joinpath(out,case["file"]);data=PM.parse_file(path)
    @assert data["bus"]["67"]["bus_type"]==3
    @assert all(g["pg"]==original["gen"][id]["pg"] for (id,g) in data["gen"])
    @assert all(length(g["references"])==1 for g in islands(data))
    PowerModelsACDC.process_additional_data!(data)
    solves=[];row_contract=[]
    for start in ("declared","flat","perturbed")
        println("Case67 contract $(case["name"]) / $start");flush(stdout)
        pm=PM.instantiate_model(deepcopy(data),PM.ACPPowerModel,PowerModelsACDC.build_acdcpf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
        vars=Dict(name(v)=>v for v in all_variables(pm.model))
        shared=shared_voltage_rows!(pm,data,vars)
        @assert length(shared)==1
        initial,extra=native_point(Dict("point"=>case["starts"][start]),pm,data,case["ids"],vars)
        for (v,x) in initial;set_start_value(v,x);end
        droop_rows=[]
        for (id,c) in sort!(collect(data["convdc"]);by=p->parse(Int,first(p)))
            c["status"]==1 && c["type_dc"]==3 || continue
            @assert c["droop"]>0 && c["dVdcset"]==0
            p=vars["0_pconv_dc[$id]"];v=vars["0_vdcm[$(c["busdc_i"])]"]
            rows=[r for r in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64}) if Set(x for (a,x) in linear_terms(constraint_object(r).func) if a!=0)==Set([p,v])]
            row=only(rows);obj=constraint_object(row)
            @assert coefficient(obj.func,p)==1
            @assert coefficient(obj.func,v)==1/c["droop"]
            expected=c["Pdcset"]+c["Vdcset"]/c["droop"]
            @assert isapprox(obj.set.value-constant(obj.func),expected;atol=1e-12,rtol=0)
            # Independent scalar equation, off the solved point and on both sides
            # of Vdcset. Check slope, power location and base conversion.
            r(pdc,vdc)=pdc-c["Pdcset"]+(vdc-c["Vdcset"])/c["droop"]
            for dv in (-0.01,0.,0.01)
                vv=c["Vdcset"]+dv;pp=c["Pdcset"]-dv/c["droop"]
                @assert abs(r(pp,vv))<1e-11
                @assert abs(value(x->x==p ? pp : vv,obj.func)-obj.set.value)<1e-11
                h=1e-6
                @assert abs((r(pp+h,vv)-r(pp-h,vv))/(2h)-1)<1e-7
                @assert abs((r(pp,vv+h)-r(pp,vv-h))/(2h)-1/c["droop"])<1e-6
            end
            push!(droop_rows,(id,row,p,v))
            if start=="declared";push!(row_contract,Dict("converter"=>id,"pdc_target_pu"=>c["Pdcset"],"vdc_target_pu"=>c["Vdcset"],"droop_pu_voltage_per_pu_power"=>c["droop"],"dPdc_dVdc"=>-1/c["droop"]));end
        end
        duplicates=deduplicate_voltage_rows!(pm)
        voltage_vars=[v for (n,v) in vars if occursin(r"^0_(vm|vmf|vmc|vdcm)\[",n)]
        bounds=Dict(v=>(has_lower_bound(v) ? lower_bound(v) : nothing) for v in voltage_vars)
        for v in voltage_vars;set_lower_bound(v,1e-8);end
        constants=Dict(vars[k]=>Float64(v["value"]) for (k,v) in extra)
        for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
            obj=constraint_object(c);terms=[(coef,v) for (coef,v) in linear_terms(obj.func) if coef!=0]
            length(terms)==1 || continue
            coef,v=only(terms);target=(obj.set.value-constant(obj.func))/coef
            @assert !haskey(constants,v)||abs(constants[v]-target)<1e-12
            constants[v]=target
        end
        saved=auxiliary_presolve!(pm,constants)
        set_optimizer(pm.model,optimizer_with_attributes(Ipopt.Optimizer,options...));optimize!(pm.model)
        status=termination_status(pm.model)
        x=has_values(pm.model) ? Dict(v=>value(v) for v in all_variables(pm.model)) : nothing
        restore_auxiliaries!(pm,saved);restore_voltage_rows!(pm,duplicates)
        for (v,bound) in bounds
            if bound===nothing;delete_lower_bound(v);else;set_lower_bound(v,bound);end
        end
        violation=x===nothing ? nothing : maximum(values(primal_feasibility_report(pm.model,x));init=0.)
        result=Dict{String,Any}("start"=>start,"termination"=>string(status),"max_reference_residual"=>violation,"duplicate_voltage_rows_restored"=>length(duplicates))
        if status in (MOI.LOCALLY_SOLVED,MOI.ALMOST_LOCALLY_SOLVED)
            @assert all(x[v]>1e-8 for v in voltage_vars)
            feasible(pm,x,Dict())
            keys_native=sort!(collect(keys(case["starts"][start])))
            markers=Dict(k=>Float64(i+1000) for (i,k) in enumerate(keys_native))
            mapped,_=native_point(Dict("point"=>markers),pm,data,case["ids"],vars)
            reverse=Dict(Int(v)-1000=>r for (r,v) in mapped if !(name(r) in keys(extra)))
            @assert length(reverse)==length(keys_native)
            result["native_point"]=Dict(k=>x[reverse[i]] for (i,k) in enumerate(keys_native))
            negative=0
            for (_,row,p,v) in droop_rows
                for (variable,delta) in ((p,0.01),(v,0.001))
                    bad=copy(x);bad[variable]+=delta
                    @assert primal_feasibility_report(pm.model,bad)[row]>0.009
                    negative+=1
                end
            end
            # Preserve bus Q balance but break the selected sharing allocation.
            gid=only(id for (id,g) in data["gen"] if g["gen_bus"]==67)
            bad=copy(x);bad[vars["0_qg[$gid]"]]+=0.01;bad[vars["0_qconv_tf_fr[9]"]]+=0.01
            @assert primal_feasibility_report(pm.model,bad)[only(shared)]>0.019
            result["negative_controls"]=negative+1
            result["droop_scalar_witnesses"]=3*length(droop_rows)
        else
            result["failed_iterate"]=x===nothing ? nothing : Dict(name(v)=>a for (v,a) in x)
        end
        push!(solves,result)
    end
    push!(cases,Dict("case"=>case["case"],"name"=>case["name"],"source_sha256"=>bytes2hex(sha256(read(path))),"islands"=>islands(data),"droop_rows"=>row_contract,"solves"=>solves))
end
result=Dict("scope"=>"case67_reference_PF_contract_and_native_electrical_control_residuals; no_native_droop_PF_solver", "source_sha256"=>CASE_SHA,"reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7","powerio_revision"=>"3e41473984585c0b38560f47ba3e0c2d930b029b","julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"../../Manifest.toml")))),"solver_options"=>options,"original_islands"=>original_islands,"cases"=>cases)
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,result,2);println(io);end
@assert all(count(s->haskey(s,"native_point"),c["solves"])>=2 for c in cases) "fewer than two successful starts; trials retained in reference.json"
println("$(sum(haskey(s,"native_point") for c in cases for s in c["solves"])) reference PF contract trials pass; failed trials retained")
