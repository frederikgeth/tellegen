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
@assert CASE_NAME in ("case5", "case24") "unqualified source case"
const CASE_DIR = CASE_NAME == "case5" ? (@__DIR__) : joinpath(@__DIR__, "case24")
const CASE = joinpath(CASE_DIR, CASE_NAME == "case5" ? "case5_3_he.m" : "case24_7_jb.m")
const CASE_SHA = CASE_NAME == "case5" ? "759606ec335f8706ee8631ff4833c50804dd916afb5b5e682ace4f4f886fd7f3" : "fe8c31d586ef6853257a0ec9a018c2b92da030808e58e57b309deac1eb5ae4b4"
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
        @assert haskey(data, old) && !haskey(data, new)
        data[new] = pop!(data, old)
    end
    return data
end
