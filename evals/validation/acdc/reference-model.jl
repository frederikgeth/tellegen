# Pinned independent reference model environment and source-case reader.
using PowerModelsACDC, JuMP, Ipopt, JSON, SHA, LinearAlgebra, TOML
const PM = PowerModelsACDC._PM
PM.silence()
BLAS.set_num_threads(1)
@assert VERSION == v"1.12.6"
@assert pkgversion(PowerModelsACDC) == v"0.10.1"
manifest = TOML.parsefile(joinpath(@__DIR__, "Manifest.toml"))
@assert only(manifest["deps"]["PowerModelsACDC"])["repo-rev"] == "9ada4082ac087aed649f48fb5f977680b0fe06c7"
const CASE_NAME = get(ENV, "TELLEGEN_ACDC_CASE", "case5")
const CASES = JSON.parsefile(joinpath(@__DIR__, "cases.json"))
@assert haskey(CASES, CASE_NAME) "unqualified source case"
const CASE_SPEC = CASES[CASE_NAME]
const CASE_DIR = CASE_NAME == "case5" ? (@__DIR__) : joinpath(@__DIR__, CASE_NAME)
const CASE = joinpath(CASE_DIR, CASE_SPEC["source_file"])
const CASE_SHA = CASE_SPEC["source_sha256"]
@assert bytes2hex(sha256(read(CASE))) == CASE_SHA
const OPTIONS = Dict{String,Any}("print_level" => 0, "sb" => "yes", "tol" => 1e-9,
    "max_iter" => 3000, "max_cpu_time" => 120.0, "bound_relax_factor" => 0.0,
    "linear_solver" => "mumps")
const SETTINGS = Dict("conv_losses_mp" => true)
const REFS = [PowerModelsACDC.add_ref_dcgrid!, PowerModelsACDC.ref_add_pst!,
    PowerModelsACDC.ref_add_sssc!, PowerModelsACDC.ref_add_flex_load!,
    PowerModelsACDC.ref_add_gendc!, PowerModelsACDC.ref_add_im!]

function source_case()
    data = PM.parse_file(CASE)
    for (old, new) in [("dcbus", "busdc"), ("dcconv", "convdc"), ("dcbranch", "branchdc")]
        @assert xor(haskey(data, old), haskey(data, new)) "missing or ambiguous DC table"
        if haskey(data, old)
            data[new] = pop!(data, old)
        end
    end
    return data
end
