function feasible(pm,x,scales)
    violations=primal_feasibility_report(pm.model,x)
    worst=maximum((v*get(scales,c,1.0) for (c,v) in violations);init=0.0)
    @assert isfinite(worst) && worst<1e-7 "reference violation $worst: $violations"
    return worst
end
function costcheck(a,b)
    @assert abs(a-b)<1e-6+1e-8*max(abs(a),abs(b)) "cost $a != $b"
end
# Scale only the explicit SI substitutions for optimization, retaining their
# original units in every feasibility check. No feasible set or bound changes.
function normalize_physical_rows!(pm,data,vars)
    scales=Dict{Any,Float64}()
    function normalize(c,scale)
        obj=constraint_object(c)
        delete(pm.model,c)
        replacement=@constraint(pm.model,obj.func/scale == obj.set.value/scale)
        scales[replacement]=scale
    end
    for (id,s) in data["convdc"]
        if s["transformer"]==0
            vp,vf=vars["0_vm[$(s["busac_i"])]"],vars["0_vmf[$id]"]
            rows=[c for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
                if Set(v for (coef,v) in linear_terms(constraint_object(c).func) if coef!=0)==Set([vp,vf])]
            normalize(only(rows),Float64(s["basekVac"]))
        end
    end
    for (id,line) in data["branchdc"]
        f,t=line["fbusdc"],line["tbusdc"]
        vf,vt=vars["0_vdcm[$f]"],vars["0_vdcm[$t]"]
        for key in ("0_pdcgrid[($id, $f, $t)]","0_pdcgrid[($id, $t, $f)]")
            flow=vars[key]
            rows=[c for c in all_constraints(pm.model,QuadExpr,MOI.EqualTo{Float64})
                if coefficient(constraint_object(c).func,flow)!=0 &&
                Set(v for (coef,v1,v2) in quad_terms(constraint_object(c).func) if coef!=0 for v in (v1,v2))==Set([vf,vt])]
            normalize(only(rows),Float64(data["baseMVA"]))
        end
    end
    return scales
end
# Exact presolve of declared constants and unused VSC firing angles only.
# Restore the original rows/bounds before accepting any solved point.
function auxiliary_presolve!(pm,constants)
    saved=[]
    for (v,target) in constants
        @assert !is_fixed(v)
        rows=[c for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
            if Set(x for (coef,x) in linear_terms(constraint_object(c).func) if coef!=0)==Set([v])]
        if startswith(name(v),"0_phiconv[")
            @assert isempty(rows)
            seen=Set{VariableRef}()
            value(x->(push!(seen,x);1.0),objective_function(pm.model))
            for c in all_constraints(pm.model;include_variable_in_set_constraints=false)
                value(x->(push!(seen,x);1.0),constraint_object(c).func)
            end
            @assert !(v in seen) "VSC firing angle must be unused"
        else
            c=constraint_object(only(rows))
            @assert isapprox((c.set.value-constant(c.func))/coefficient(c.func,v),target;atol=1e-12,rtol=0)
        end
        bounds=(has_lower_bound(v) ? lower_bound(v) : nothing,has_upper_bound(v) ? upper_bound(v) : nothing)
        objects=constraint_object.(rows)
        for c in rows;delete(pm.model,c);end
        fix(v,target;force=true)
        push!(saved,(v,bounds,objects))
    end
    @assert length(saved)==length(constants)
    return saved
end
function restore_auxiliaries!(pm,saved)
    for (v,(lo,hi),rows) in saved
        unfix(v)
        if lo !== nothing;set_lower_bound(v,lo);end
        if hi !== nothing;set_upper_bound(v,hi);end
        for c in rows;@constraint(pm.model,c.func==c.set.value);end
    end
end
