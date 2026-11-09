# Independent oracle: original source -> PowerModelsACDC ACP PF/OPF.
include("reference-model.jl")
data = source_case()
snapshot(d) = Dict(k => deepcopy(d[k]) for k in ("baseMVA", "dcpol", "busdc", "branchdc", "convdc"))
declared = snapshot(data)
PowerModelsACDC.process_additional_data!(data)
effective = snapshot(data)
records = Dict{String,Any}()
for (mode, builder) in [("pf", PowerModelsACDC.build_acdcpf), ("opf", PowerModelsACDC.build_acdcopf)]
    pm = PM.instantiate_model(deepcopy(data), PM.ACPPowerModel, builder;
        ref_extensions=REFS, setting=deepcopy(SETTINGS))
    starts = Dict(name(v) => start_value(v) for v in all_variables(pm.model))
    result = PM.optimize_model!(pm; optimizer=optimizer_with_attributes(Ipopt.Optimizer, collect(OPTIONS)...))
    @assert result["termination_status"] in (MOI.LOCALLY_SOLVED, MOI.OPTIMAL)
    @assert has_values(pm.model)
    worst = maximum(values(primal_feasibility_report(pm.model)); init=0.0)
    @assert isfinite(worst) && worst <= 1e-7
    records[mode] = Dict("termination_status" => string(result["termination_status"]),
        "primal_status" => string(primal_status(pm.model)), "objective" => result["objective"],
        "max_primal_violation" => worst, "starts" => starts, "solution" => result["solution"])
end
output = Dict("source_sha256" => CASE_SHA, "powermodelsacdc_commit" => "9ada4082ac087aed649f48fb5f977680b0fe06c7",
    "julia" => string(VERSION), "powermodelsacdc" => string(pkgversion(PowerModelsACDC)),
    "powermodels" => string(pkgversion(PM)), "jump" => string(pkgversion(JuMP)), "ipopt" => string(pkgversion(Ipopt)),
    "manifest_sha256" => bytes2hex(sha256(read(joinpath(@__DIR__, "Manifest.toml")))),
    "settings" => SETTINGS, "optimizer_options" => OPTIONS, "declared_acdc" => declared,
    "effective_acdc" => effective, "solves" => records)
path = isempty(ARGS) ? joinpath(@__DIR__, "reference.json") : only(ARGS)
open(path, "w") do io
    JSON.print(io, output, 2)
    println(io)
end
println("Saved independently solved PF/OPF points to ", path)
