# Evaluate physical phasor points in independently built pinned reference models.
include("../reference-model.jl")
@assert length(ARGS) == 1 "usage: check.jl OUTPUT_DIRECTORY"
out = abspath(ARGS[1])
cases = JSON.parsefile(joinpath(out, "cases.json"))
selection = only(unique(case["profile"] for case in cases))
@assert selection in ("compatibility", "declared-current")
suite = only(unique(get(case,"suite","physical") for case in cases))
@assert suite in ("physical", "bypass")
@assert length(cases) == (suite == "bypass" ? 384 : selection == "compatibility" ? 42 : 108)
records = []
for case in cases
    path = joinpath(out, case["file"])
    data = PM.parse_file(path)
    @assert Set(keys(data["bus"])) == Set(["11"])
    @assert Set(keys(data["gen"])) == Set(["1"])
    @assert data["gen"]["1"]["gen_bus"] == 11
    @assert isempty(data["branch"]) && isempty(data["load"])
    @assert Set(bus["busdc_i"] for bus in values(data["busdc"])) == Set([21,22])
    @assert Set(keys(data["branchdc"])) == Set(["1"])
    @assert Set(keys(data["convdc"])) == Set(["1"])
    @assert (data["branchdc"]["1"]["fbusdc"], data["branchdc"]["1"]["tbusdc"]) == (21,22)
    @assert (data["convdc"]["1"]["busac_i"], data["convdc"]["1"]["busdc_i"]) == (11,21)
    @assert data["dcpol"] == case["poles"]
    b = case["bases"]
    @assert data["baseMVA"] == b["mva"]
    @assert data["bus"]["11"]["base_kv"] == b["pcc_kv"]
    @assert data["convdc"]["1"]["basekVac"] == b["converter_kv"]
    @assert all(bus["basekVdc"] == (bus["busdc_i"] == 22 ? get(case,"dc_to_base_kv",b["dc_kv"]) : b["dc_kv"]) for bus in values(data["busdc"]))
    if suite == "bypass"
        for (flag, key) in [("transformer","transformer"),("reactor","reactor"),("filter","filter")]
            @assert data["convdc"]["1"][key] == case["elements"][flag]
        end
    end
    ibase = b["mva"] / (sqrt(3)*b["converter_kv"])
    declared = data["convdc"]["1"]["Imax"] * ibase
    @assert isapprox(declared, case["declared_current_limit_ka"]; atol=1e-12)
    declared_pu = data["convdc"]["1"]["Imax"]
    PowerModelsACDC.process_additional_data!(data)
    repaired = data["convdc"]["1"]["Imax"] * ibase
    if selection == "declared-current"
        # Explicitly modified reference: undo ONLY the Imax repair, before model construction.
        # Imax is already pu in the source and is not rescaled by the preprocessor.
        data["convdc"]["1"]["Imax"] = declared_pu
    end
    @assert Set(keys(data["busdc"])) == Set(["21", "22"])
    effective = data["convdc"]["1"]["Imax"] * ibase
    @assert isapprox(effective, case["effective_current_limit_ka"]; atol=1e-12)
    if selection == "declared-current"
        @assert isapprox(effective, declared; atol=1e-12)
    end
    pm = PM.instantiate_model(data, PM.ACPPowerModel, PowerModelsACDC.build_acdcopf;
        ref_extensions=REFS, setting=deepcopy(SETTINGS))
    vars = Dict(name(v)=>v for v in all_variables(pm.model))
    x = case["point"]
    mapped = Dict{VariableRef,Float64}()
    consumed = Set{String}()
    function put(refname, nativename)
        @assert haskey(vars, refname) && haskey(x, nativename) "$refname / $nativename"
        @assert !haskey(mapped, vars[refname]) && !(nativename in consumed)
        @assert isfinite(x[nativename])
        mapped[vars[refname]] = x[nativename]
        push!(consumed, nativename)
    end
    for f in ("vm", "va")
        put("0_$f[11]", "$f[11]")
    end
    for f in ("pg", "qg")
        put("0_$f[1]", "$f[physical-generator]")
    end
    for id in (21,22)
        put("0_vdcm[$id]", "dc_bus/dc_equivalent_bus/$id/vm")
    end
    put("0_pdcgrid[(1, 21, 22)]", "dc_branch/dc_equivalent_branch/41/pf")
    put("0_pdcgrid[(1, 22, 21)]", "dc_branch/dc_equivalent_branch/41/pt")
    fields = Dict("pgrid"=>"pconv_tf_fr", "qgrid"=>"qconv_tf_fr",
        "ptf_to"=>"pconv_tf_to", "qtf_to"=>"qconv_tf_to", "ppr_fr"=>"pconv_pr_fr",
        "qpr_fr"=>"qconv_pr_fr", "pconv"=>"pconv_ac", "qconv"=>"qconv_ac", "pdc"=>"pconv_dc",
        "iconv"=>"iconv_ac", "vmfilt"=>"vmf", "vafilt"=>"vaf", "vmconv"=>"vmc", "vaconv"=>"vac")
    for (f,rf) in fields
        put("0_$rf[1]", "station/ac_dc_converter_station/31/$f")
    end
    @assert consumed == Set(keys(x))
    @assert data["convdc"]["1"]["islcc"] == 0
    mapped[vars["0_phiconv[1]"]] = 0.0 # Unused VSC firing angle, not an oracle solution.
    @assert Set(keys(mapped)) == Set(all_variables(pm.model))
    violations = primal_feasibility_report(pm.model, mapped)
    worst = maximum(values(violations); init=0.0)
    current_bound = UpperBoundRef(vars["0_iconv_ac[1]"])
    expected_constraints = Dict{String,Any}()
    if suite == "bypass" && case["kind"] == "unequal_pcc_bypass"
        matches = [c for c in all_constraints(pm.model, AffExpr, MOI.EqualTo{Float64})
            if Set(v for (coef,v) in linear_terms(constraint_object(c).func) if coef != 0) == Set([vars["0_vm[11]"],vars["0_vmf[1]"]])]
        expected_constraints["station/ac_dc_converter_station/31/transformer/vm_bypass"] = only(matches)
    elseif suite == "bypass" && case["kind"] == "unequal_dc_bases"
        for (suffix, refname) in [("pf","0_pdcgrid[(1, 21, 22)]"),("pt","0_pdcgrid[(1, 22, 21)]")]
            matches = [c for c in all_constraints(pm.model, QuadExpr, MOI.EqualTo{Float64})
                if coefficient(constraint_object(c).func,vars[refname]) != 0 &&
                   Set(v for (coef,v1,v2) in quad_terms(constraint_object(c).func) if coef != 0 for v in (v1,v2)) == Set([vars["0_vdcm[21]"],vars["0_vdcm[22]"]])]
            expected_constraints["dc_branch/dc_equivalent_branch/41/$suffix"] = only(matches)
        end
    end
    if suite == "bypass"
        @assert Set(keys(expected_constraints)) == Set(case["expected_violations"])
        @assert Set(c for (c,v) in violations if v > 1e-9) == Set(values(expected_constraints))
        for (name,c) in expected_constraints
            @assert isapprox(violations[c],case["native_violations"][name]; atol=1e-9,rtol=1e-12)
        end
    end
    ignored = suite == "bypass" ? Set(values(expected_constraints)) : Set([current_bound])
    other = maximum((v for (c,v) in violations if !(c in ignored)); init=0.0)
    @assert isfinite(other) && other < 1e-9 "$(case["file"]): $violations"
    @assert isapprox(worst, case["native_max_violation"]; atol=1e-9)
    if case["expected_feasible"]
        @assert worst < 1e-9
    elseif suite != "bypass"
        @assert get(violations, current_bound, 0.0) > 0.1
    end
    current_violation_ka = get(violations, current_bound, 0.0) * ibase
    if selection == "declared-current"
        @assert isapprox(current_violation_ka, max(case["physical_current_ka"]-declared,0.0); atol=1e-12)
    end
    objective = value(v -> mapped[v], objective_function(pm.model))
    @assert isapprox(objective, case["objective"]; atol=1e-9)
    # Negative control: confusing phase and three-phase current must be detected.
    corrupted = copy(mapped)
    corrupted[vars["0_iconv_ac[1]"]] *= sqrt(3)
    corrupted_report = primal_feasibility_report(pm.model, corrupted)
    detected = maximum(values(corrupted_report); init=0.0)
    @assert detected > 1e-3
    # A pre-existing expected violation must not make the corruption control pass.
    current_variables = Set(vars[n] for n in ("0_pconv_ac[1]","0_qconv_ac[1]","0_vmc[1]","0_iconv_ac[1]"))
    current_equations = []
    for c in all_constraints(pm.model; include_variable_in_set_constraints=false)
        seen = Set{VariableRef}()
        value(v -> (push!(seen,v); mapped[v]), constraint_object(c).func)
        if seen == current_variables
            push!(current_equations,c)
        end
    end
    current_equation = only(current_equations)
    @assert get(violations,current_equation,0.0) < 1e-9
    current_corruption = get(corrupted_report,current_equation,0.0)
    @assert current_corruption > 1e-3
    push!(records, Dict("file"=>case["file"], "source_sha256"=>bytes2hex(sha256(read(path))),
        "bases"=>b, "direction"=>case["direction"], "poles"=>case["poles"],
        "declared_current_limit_ka"=>declared, "effective_current_limit_ka"=>effective,
        "reference_repaired_current_limit_ka"=>repaired, "current_bound_violation_ka"=>current_violation_ka,
        "physical_current_ka"=>case["physical_current_ka"], "expected_feasible"=>case["expected_feasible"],
        "max_reference_violation"=>worst, "max_other_violation"=>other,
        "objective_difference"=>objective-case["objective"], "negative_control_violation"=>detected,
        "native_variables"=>length(x), "reference_variables"=>length(mapped)))
    if suite == "bypass"
        records[end]["kind"] = case["kind"]
        records[end]["elements"] = case["elements"]
        records[end]["dc_to_base_kv"] = case["dc_to_base_kv"]
        records[end]["expected_violations"] = Dict(name=>violations[c] for (name,c) in expected_constraints)
        records[end]["current_equation_corruption_violation"] = current_corruption
    end
end
output = Dict("suite"=>suite,"profile"=>selection,
    "reference_modification"=>(selection == "declared-current" ? "restore_source_Imax_after_preprocessing_before_instantiation" : "none"),
    "reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7",
    "powerio_revision"=>"896fef026feceb4cc2111a4914100fe313986386",
    "manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__, "..", "Manifest.toml")))),
    "cases_sha256"=>bytes2hex(sha256(read(joinpath(out, "cases.json")))),
    "julia_version"=>string(VERSION), "records"=>records)
open(joinpath(out, "summary.json"), "w") do io
    JSON.print(io, output, 2); println(io)
end
println("$(length(cases)) $selection cases checked; expected outcomes and remaining constraints pass")
