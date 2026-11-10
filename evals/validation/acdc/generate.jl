# Independent oracle: original source -> PowerModelsACDC ACP PF/OPF.
include("reference-model.jl")
data = source_case()
snapshot(d) = Dict(k => deepcopy(d[k]) for k in ("baseMVA", "dcpol", "busdc", "branchdc", "convdc"))
declared = snapshot(data)
PowerModelsACDC.process_additional_data!(data)
effective = snapshot(data)
# Probe the unconstrained common phase of AC islands without a declared
# reference. DC edges do not couple AC phase. Source/model constraints remain
# unchanged; shift each island's bus, filter and valve angles together.
function angle_gauge_probe(pm, data)
    components = sort!(collect(PM.calc_connected_components(data; edges=["branch", "switch"])); by=minimum)
    vars = Dict(name(v) => v for v in all_variables(pm.model))
    original = Dict(v => value(v) for v in all_variables(pm.model))
    original_objective = value(v -> original[v], objective_function(pm.model))
    records = []
    for component in components
        refs = sort!([i for i in component if data["bus"][string(i)]["bus_type"] == 3])
        record = Dict{String,Any}("buses" => sort!(collect(component)), "reference_buses" => refs)
        if isempty(refs)
            names = ["0_va[$i]" for i in sort!(collect(component))]
            for (id, station) in data["convdc"]
                if station["status"] == 1 && station["busac_i"] in component
                    append!(names, ["0_vaf[$id]", "0_vac[$id]"])
                end
            end
            shifts = []
            for delta in (-0.05, 0.05)
                shifted = copy(original)
                for key in names
                    shifted[vars[key]] += delta
                end
                worst = maximum(values(primal_feasibility_report(pm.model, shifted)); init=0.0)
                difference = value(v -> shifted[v], objective_function(pm.model)) - original_objective
                @assert isfinite(worst) && worst <= 1e-7
                @assert abs(difference) <= 1e-7
                push!(shifts, Dict("shift_rad" => delta, "max_primal_violation" => worst,
                    "objective_difference" => difference))
            end
            record["shifted_variables"] = sort!(names)
            record["probes"] = shifts
        end
        push!(records, record)
    end
    return records
end

records = Dict{String,Any}()
for (mode, builder) in [("pf", PowerModelsACDC.build_acdcpf), ("opf", PowerModelsACDC.build_acdcopf)]
    pm = PM.instantiate_model(deepcopy(data), PM.ACPPowerModel, builder;
        ref_extensions=REFS, setting=deepcopy(SETTINGS))
    starts = Dict(name(v) => start_value(v) for v in all_variables(pm.model))
    result = PM.optimize_model!(pm; optimizer=optimizer_with_attributes(Ipopt.Optimizer, collect(OPTIONS)...))
    qualified = result["termination_status"] in (MOI.LOCALLY_SOLVED, MOI.OPTIMAL)
    if !qualified
        @assert mode == "pf" && string(result["termination_status"]) == get(CASE_SPEC, "expected_pf_failure", nothing)
        @warn "Reference PF is not qualified; retaining local failure, not an infeasibility proof" case=CASE_NAME status=result["termination_status"]
    end
    @assert has_values(pm.model)
    worst = maximum(values(primal_feasibility_report(pm.model)); init=0.0)
    @assert isfinite(worst)
    @assert !qualified || worst <= 1e-7
    records[mode] = Dict("qualified" => qualified, "termination_status" => string(result["termination_status"]),
        "primal_status" => string(primal_status(pm.model)), "objective" => result["objective"],
        "max_primal_violation" => worst, "starts" => starts, "solution" => result["solution"])
    if mode == "opf"
        records[mode]["ac_angle_gauge"] = angle_gauge_probe(pm, data)
    end
end
output = Dict("source_sha256" => CASE_SHA, "powermodelsacdc_commit" => "9ada4082ac087aed649f48fb5f977680b0fe06c7",
    "julia" => string(VERSION), "powermodelsacdc" => string(pkgversion(PowerModelsACDC)),
    "powermodels" => string(pkgversion(PM)), "jump" => string(pkgversion(JuMP)), "ipopt" => string(pkgversion(Ipopt)),
    "manifest_sha256" => bytes2hex(sha256(read(joinpath(@__DIR__, "Manifest.toml")))),
    "settings" => SETTINGS, "optimizer_options" => OPTIONS, "declared_acdc" => declared,
    "effective_acdc" => effective, "effective_bus_types" => Dict(id=>b["bus_type"] for (id,b) in data["bus"]), "solves" => records)
path = isempty(ARGS) ? joinpath(CASE_DIR, "reference.json") : only(ARGS)
open(path, "w") do io
    JSON.print(io, output, 2)
    println(io)
end
println("Saved independently solved PF/OPF points to ", path)
