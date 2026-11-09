# Independent oracle: original MATPOWER-extension case -> PowerModelsACDC ACP.
# No Tellegen expressions, PowerIO export, or native residual formulas are used.
using PowerModelsACDC, JuMP, Ipopt, JSON, SHA, LinearAlgebra, TOML
const PM = PowerModelsACDC._PM
PM.silence()
BLAS.set_num_threads(1)
@assert VERSION == v"1.12.6"
@assert pkgversion(PowerModelsACDC) == v"0.10.1"
manifest = TOML.parsefile(joinpath(@__DIR__, "Manifest.toml"))
@assert only(manifest["deps"]["PowerModelsACDC"])["repo-rev"] == "9ada4082ac087aed649f48fb5f977680b0fe06c7"
const CASE = joinpath(@__DIR__, "case5_3_he.m")
const CASE_SHA = "759606ec335f8706ee8631ff4833c50804dd916afb5b5e682ace4f4f886fd7f3"
@assert bytes2hex(sha256(read(CASE))) == CASE_SHA
const OPTIONS = Dict{String,Any}("print_level" => 0, "sb" => "yes", "tol" => 1e-9,
    "max_iter" => 3000, "max_cpu_time" => 120.0, "bound_relax_factor" => 0.0,
    "linear_solver" => "mumps")
const SETTINGS = Dict("conv_losses_mp" => true)
const REFS = [PowerModelsACDC.add_ref_dcgrid!, PowerModelsACDC.ref_add_pst!,
    PowerModelsACDC.ref_add_sssc!, PowerModelsACDC.ref_add_flex_load!,
    PowerModelsACDC.ref_add_gendc!, PowerModelsACDC.ref_add_im!]
data = PM.parse_file(CASE)
for (old, new) in [("dcbus", "busdc"), ("dcconv", "convdc"), ("dcbranch", "branchdc")]
    @assert haskey(data, old) && !haskey(data, new)
    data[new] = pop!(data, old)
end
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
