# Complete native/reference identity mapping, including declared reference-only auxiliaries.
function native_point(result,pm,data,ids,vars)
    x = result["point"]
    mapped = Dict{VariableRef,Float64}()
    consumed = Set{String}()
    function put(refname, nativename)
        @assert haskey(vars, refname) && haskey(x, nativename) "$refname / $nativename"
        @assert !haskey(mapped, vars[refname])
        @assert isfinite(x[nativename])
        mapped[vars[refname]] = x[nativename]
        push!(consumed, nativename)
    end
    for (id, refid) in ids["bus"], field in ("vm", "va")
        put("0_$(field)[$refid]", "$(field)[$id]")
    end
    for (id, refid) in ids["gen"], field in ("pg", "qg")
        put("0_$(field)[$refid]", "$(field)[$id]")
    end
    for (id, refid) in ids["branch"]
        b = data["branch"][refid]
        for (suffix, f, t) in [("f", b["f_bus"], b["t_bus"]), ("t", b["t_bus"], b["f_bus"])], field in ("p", "q")
            put("0_$(field)[($(refid), $(f), $(t))]", "ac_branch/$id/$(field)$(suffix)")
        end
    end
    for (id, refid) in ids["busdc"]
        put("0_vdcm[$refid]", "dc_bus/$id/vm")
    end
    for (id, refid) in ids["branchdc"]
        b = data["branchdc"][refid]
        for (suffix, f, t) in [("f", b["fbusdc"], b["tbusdc"]), ("t", b["tbusdc"], b["fbusdc"])]
            put("0_pdcgrid[($(refid), $(f), $(t))]", "dc_branch/$id/p$(suffix)")
        end
    end
    converter_fields = Dict("pgrid"=>"pconv_tf_fr", "qgrid"=>"qconv_tf_fr",
        "ptf_to"=>"pconv_tf_to", "qtf_to"=>"qconv_tf_to", "ppr_fr"=>"pconv_pr_fr",
        "qpr_fr"=>"qconv_pr_fr", "pconv"=>"pconv_ac", "qconv"=>"qconv_ac", "pdc"=>"pconv_dc",
        "iconv"=>"iconv_ac", "vmfilt"=>"vmf", "vafilt"=>"vaf", "vmconv"=>"vmc", "vaconv"=>"vac")
    for (id, refid) in ids["convdc"], (field, reffield) in converter_fields
        put("0_$(reffield)[$refid]", "station/$id/$field")
    end
    @assert consumed == Set(keys(x)) "Every native coordinate must be mapped"
    # Reference-only variables: constant demand and unused VSC firing angles.
    # Reconstruct from source declarations, never from the oracle's solved point.
    reconstructed = Dict{String,Any}()
    for (id, load) in data["load"]
        @assert load["flex"] == 0 "Only fixed loads qualified in this fixture"
        for (field, v) in [("pflex", load["pd"]), ("qflex", load["qd"]), ("pcurt", 0.0)]
            name = "0_$(field)[$id]"
            @assert haskey(vars, name) && !haskey(mapped, vars[name])
            mapped[vars[name]] = v
            reconstructed[name] = Dict("value"=>v, "reason"=>"fixed source demand; zero curtailment")
        end
    end
    for (id, conv) in data["convdc"]
        @assert conv["islcc"] == 0
        name = "0_phiconv[$id]"
        @assert haskey(vars, name) && !haskey(mapped, vars[name])
        mapped[vars[name]] = 0.0
        reconstructed[name] = Dict("value"=>0.0, "reason"=>"unused VSC firing angle")
    end
    @assert Set(keys(mapped)) == Set(all_variables(pm.model)) "No unmapped reference variables permitted"
    return mapped, reconstructed
end

