# Replace only the three equation families whose physical coordinates differ.
# The oracle uses actual kV, MW and ohms, independently of native ratio coefficients.
function physical_equations!(pm, data, vars)
    replacements = []
    for (id,s) in data["convdc"]
        if !Bool(s["transformer"])
            vp = vars["0_vm[$(s["busac_i"])]"]
            vf = vars["0_vmf[$id]"]
            original = only([c for c in all_constraints(pm.model, AffExpr, MOI.EqualTo{Float64})
                if Set(v for (coef,v) in linear_terms(constraint_object(c).func) if coef != 0) == Set([vp,vf])])
            delete(pm.model,original)
            pcc_base = data["bus"][string(s["busac_i"])]["base_kv"]
            station_base = s["basekVac"]
            @constraint(pm.model, pcc_base*vp == station_base*vf)
            push!(replacements, "physical_transformer_bypass/$id")
        end
    end
    for (id,line) in data["branchdc"]
        f,t = line["fbusdc"],line["tbusdc"]
        vf,vt = vars["0_vdcm[$f]"],vars["0_vdcm[$t]"]
        pf,pt = vars["0_pdcgrid[($id, $f, $t)]"],vars["0_pdcgrid[($id, $t, $f)]"]
        for flow in (pf,pt)
            original=only([c for c in all_constraints(pm.model, QuadExpr, MOI.EqualTo{Float64})
                if coefficient(constraint_object(c).func,flow) != 0 &&
                   Set(v for (coef,v1,v2) in quad_terms(constraint_object(c).func) if coef != 0 for v in (v1,v2)) == Set([vf,vt])])
            delete(pm.model,original)
        end
        from_base,to_base = data["busdc"][string(f)]["basekVdc"],data["busdc"][string(t)]["basekVdc"]
        mva,poles = data["baseMVA"],data["dcpol"]
        resistance_ohm = line["r"]*from_base^2/mva
        @constraint(pm.model, mva*pf == poles*(from_base*vf)*(from_base*vf-to_base*vt)/resistance_ohm)
        @constraint(pm.model, mva*pt == poles*(to_base*vt)*(to_base*vt-from_base*vf)/resistance_ohm)
        push!(replacements,"physical_dc_ohms/$id/from", "physical_dc_ohms/$id/to")
    end
    return sort!(replacements)
end
