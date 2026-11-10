# Evaluate native solved points in the independently instantiated Julia model.
# No solver is invoked and no native equation formulas are reused here.
include("reference-model.jl")
@assert length(ARGS) in (2, 3) "usage: cross-check.jl NATIVE_RESULT OUTPUT [REFERENCE]"
native = JSON.parsefile(ARGS[1])
ids = JSON.parsefile(joinpath(CASE_DIR, "identity-map.json"))
reference_path = length(ARGS) == 3 ? ARGS[3] : joinpath(CASE_DIR, "reference.json")
reference = JSON.parsefile(reference_path)
@assert reference["source_sha256"] == CASE_SHA
@assert reference["manifest_sha256"] == bytes2hex(sha256(read(joinpath(@__DIR__, "Manifest.toml"))))
@assert native["preparation"]["reference_revision"] == "9ada4082ac087aed649f48fb5f977680b0fe06c7"
@assert native["preparation"]["powerio_revision"] == "e5a7c8b82845e32de7427f39ff8e63768f5f8bd2"
data = source_case()
PowerModelsACDC.process_additional_data!(data)
pm = PM.instantiate_model(data, PM.ACPPowerModel, PowerModelsACDC.build_acdcopf;
    ref_extensions=REFS, setting=deepcopy(SETTINGS))
vars = Dict(name(v) => v for v in all_variables(pm.model))
@assert length(vars) == length(all_variables(pm.model))

# Independently derive the expected angle references from source AC components.
# The selected policy is explicit; the native manifest cannot invent its anchors.
policy = native["preparation"]["angle_reference_policy"]
@assert policy in ("declared", "lowest_bus_id_per_island")
source_to_native = Dict(parse(Int, v) => parse(Int, k) for (k,v) in ids["bus"])
expected_references = []
angle_alignment = []
comparison_solution = deepcopy(reference["solves"]["opf"]["solution"])
for component in sort!(collect(PM.calc_connected_components(data; edges=["branch", "switch"])); by=minimum)
    refs = sort!([i for i in component if data["bus"][string(i)]["bus_type"] == 3])
    declared = !isempty(refs)
    if !declared
        @assert policy == "lowest_bus_id_per_island" "uncovered AC island needs explicit policy"
        refs = [argmin(i -> source_to_native[i], collect(component))]
    end
    for bus in refs
        push!(expected_references, Dict("bus"=>source_to_native[bus],
            "island_buses"=>sort!([source_to_native[i] for i in component]), "declared"=>declared))
        if !declared
            shift = -comparison_solution["bus"][string(bus)]["va"]
            variables = []
            for i in component
                comparison_solution["bus"][string(i)]["va"] += shift
                push!(variables, "va[$(source_to_native[i])]")
            end
            for (native_id, source_id) in ids["convdc"]
                if data["convdc"][source_id]["busac_i"] in component
                    for field in ("vafilt", "vaconv")
                        comparison_solution["convdc"][source_id][field] += shift
                        push!(variables, "station/$native_id/$field")
                    end
                end
            end
            push!(angle_alignment, Dict("anchor_bus"=>source_to_native[bus],
                "shift_radians"=>shift, "variables"=>sort!(variables)))
        end
    end
end
sort!(expected_references; by=r -> r["bus"])
@assert expected_references == native["preparation"]["angle_references"]
@assert angle_alignment == native["reference_angle_alignment"]

function native_point(result)
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

function electrical_comparison(result)
    x = result["point"]
    source = comparison_solution
    compared = Dict{String,Any}()
    put(name, actual, expected) = (compared[name] = Dict("native"=>actual, "reference"=>expected, "absolute_difference"=>abs(actual-expected)))
    for (table, fields) in [("bus", ("vm", "va")), ("gen", ("pg", "qg")),
        ("branch", ("pf", "qf", "pt", "qt")), ("busdc", ("vm",)), ("branchdc", ("pf", "pt")),
        ("convdc", ("pgrid", "qgrid", "ptf_to", "qtf_to", "ppr_fr", "qpr_fr", "pconv", "qconv", "pdc", "iconv", "vmfilt", "vafilt", "vmconv", "vaconv"))]
        for (id, refid) in ids[table]
            prefix = get(Dict("branch"=>"ac_branch", "busdc"=>"dc_bus", "branchdc"=>"dc_branch", "convdc"=>"station"), table, "")
            for field in fields
                key = table in ("bus", "gen") ? "$field[$id]" : "$prefix/$id/$field"
                put(key, x[key], source[table][refid][field])
            end
            if table in ("branch", "branchdc")
                put("$prefix/$id/active_loss", x["$prefix/$id/pf"]+x["$prefix/$id/pt"], source[table][refid]["pf"]+source[table][refid]["pt"])
            elseif table == "convdc"
                for (label, field) in [("valve_loss", "pconv"), ("station_loss", "pgrid")]
                    put("$prefix/$id/$label", x["$prefix/$id/$field"]+x["$prefix/$id/pdc"], source[table][refid][field]+source[table][refid]["pdc"])
                end
            end
        end
    end
    return compared
end

records = Dict{String,Any}()
for (label, result) in native["solves"]
    @assert result["model_fingerprint"] == native["preparation"]["model_fingerprint"]
    point, reconstructed = native_point(result)
    feasibility = primal_feasibility_report(pm.model, point)
    worst = maximum(values(feasibility); init=0.0)
    objective = value(v -> point[v], objective_function(pm.model))
    native_objective = result["validation"]["objective"]
    oracle_objective = reference["solves"]["opf"]["objective"]
    @assert isfinite(worst) && worst <= 1e-7 "$label: reference violation $worst"
    @assert isapprox(objective, native_objective; atol=1e-7, rtol=1e-10)
    @assert isapprox(objective, oracle_objective; atol=1e-7, rtol=1e-8)
    compared = electrical_comparison(result)
    aligned_oracle = Dict("point"=>Dict(k=>compared[k]["reference"] for k in keys(result["point"])))
    aligned_point, _ = native_point(aligned_oracle)
    aligned_violation = maximum(values(primal_feasibility_report(pm.model, aligned_point)); init=0.0)
    @assert isfinite(aligned_violation) && aligned_violation <= 1e-7
    @assert isapprox(value(v -> aligned_point[v], objective_function(pm.model)), oracle_objective; atol=1e-7, rtol=1e-10)

    records[label] = Dict("aligned_reference_violation"=>aligned_violation, "electrical_comparison"=>compared,
        "max_absolute_electrical_difference"=>maximum(v["absolute_difference"] for v in values(compared)),"max_reference_violation"=>worst, "reference_evaluated_objective"=>objective,
        "native_objective_difference"=>objective-native_objective,
        "oracle_objective_difference"=>objective-oracle_objective,
        "native_variables"=>length(result["point"]), "reference_variables"=>length(point),
        "reconstructed_reference_only"=>reconstructed,
        "constraint_violations"=>Dict(string(c)=>v for (c,v) in feasibility))
end
# Negative controls demonstrate that native errors do not pass by construction.
negative_controls = Dict{String,Float64}()
for key in ("station/ac_dc_converter_station/1/pgrid", "station/ac_dc_converter_station/1/qgrid", "station/ac_dc_converter_station/1/pdc", "dc_bus/dc_equivalent_bus/1/vm")
    corrupted = deepcopy(first(values(native["solves"])))
    corrupted["point"][key] += 0.1
    point, _ = native_point(corrupted)
    violation = maximum(values(primal_feasibility_report(pm.model, point)); init=0.0)
    @assert violation > 1e-3 "reference checker did not detect $key corruption"
    negative_controls[key] = violation
end
# Rotating only a bus must not pass the original station equations.
for alignment in angle_alignment
    key = "va[$(alignment["anchor_bus"])]"
    corrupted = deepcopy(first(values(native["solves"])))
    corrupted["point"][key] += 0.05
    point, _ = native_point(corrupted)
    violation = maximum(values(primal_feasibility_report(pm.model, point)); init=0.0)
    @assert violation > 1e-3
    negative_controls[key] = violation
end
output = Dict("reference_angle_alignment"=>angle_alignment, "negative_controls_detected"=>negative_controls, "source_sha256"=>CASE_SHA, "native_result_sha256"=>bytes2hex(sha256(read(ARGS[1]))),
    "reference_result_sha256"=>bytes2hex(sha256(read(reference_path))),
    "manifest_sha256"=>reference["manifest_sha256"],
    "reference_revision"=>native["preparation"]["reference_revision"], "solves"=>records)
open(ARGS[2], "w") do io
    JSON.print(io, output, 2); println(io)
end
println("All native points pass independent reference feasibility and objective checks")
