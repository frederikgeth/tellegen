# Explicit injection-increment allocation, independently formed from parsed data.
function shared_voltage_rows!(pm,data,vars)
    rows=[]
    for (id,bus) in data["bus"]
        gens=sort!([g for (g,s) in data["gen"] if s["gen_status"]==1 && s["gen_bus"]==parse(Int,id)];by=x->parse(Int,x))
        convs=sort!([c for (c,s) in data["convdc"] if s["status"]==1 && s["busac_i"]==parse(Int,id) && s["type_ac"]==2];by=x->parse(Int,x))
        for (j,c) in enumerate(convs)
            q=vars["0_qconv_tf_fr[$c]"]
            if !isempty(gens)
                g=first(gens)
                # Input Q_g is generation-positive; qconv_tf_fr is withdrawal.
                row=@constraint(pm.model,vars["0_qg[$g]"]+q==data["gen"][g]["qg"]-data["convdc"][c]["Q_g"])
            else
                j==1 && continue
                first_c=first(convs)
                row=@constraint(pm.model,q-vars["0_qconv_tf_fr[$first_c]"]==data["convdc"][first_c]["Q_g"]-data["convdc"][c]["Q_g"])
            end
            push!(rows,row)
        end
    end
    rows
end
# PMACDC inserts one Vm equality for each voltage controller. Remove only
# algebraically identical duplicates for optimization; restore before validation.
function deduplicate_voltage_rows!(pm)
    seen=Dict{VariableRef,Float64}();removed=[]
    for c in all_constraints(pm.model,AffExpr,MOI.EqualTo{Float64})
        obj=constraint_object(c)
        terms=[(coef,v) for (coef,v) in linear_terms(obj.func) if coef!=0]
        length(terms)==1 || continue
        coef,v=only(terms)
        startswith(name(v),"0_vm[") || continue
        target=(obj.set.value-constant(obj.func))/coef
        if haskey(seen,v)
            @assert target==seen[v] "Different voltage targets may not be deduplicated"
            push!(removed,obj);delete(pm.model,c)
        else
            seen[v]=target
        end
    end
    removed
end
function restore_voltage_rows!(pm,rows)
    for c in rows;@constraint(pm.model,c.func==c.set.value);end
end
