# Independent pinned ACP loss-row characterization; no directional OPF solve.
include("../reference-model.jl")
include("reference-physical.jl")
@assert length(ARGS)==1
out=abspath(only(ARGS))
cases=JSON.parsefile(joinpath(out,"cases.json"))
@assert length(cases)==1728
records=[]
for case in cases
    path=joinpath(out,case["file"])
    data=PM.parse_file(path)
    b=case["bases"]
    spec=case["case"]
    raw=deepcopy(data["convdc"]["1"])
    @assert (raw["LossCrec"],raw["LossCinv"])==Tuple(spec["coefficients"])
    @assert isapprox(raw["P_g"],-case["pcc_mw"];atol=1e-10)
    @assert raw["LossA"]==0.4 && raw["LossB"]==0.7
    @assert data["gen"]["1"]["gen_bus"]==11
    @assert (raw["busac_i"],raw["busdc_i"])==(11,21)
    @assert (data["branchdc"]["1"]["fbusdc"],data["branchdc"]["1"]["tbusdc"])==(21,22)
    @assert data["dcpol"]==spec["poles"]
    PowerModelsACDC.process_additional_data!(data)
    data["convdc"]["1"]["Imax"]=raw["Imax"]
    ibase=b["mva"]/(sqrt(3)*b["converter_kv"])
    conv=data["convdc"]["1"]
    for key in ("LossCrec","LossCinv")
        @assert isapprox(conv[key]*b["mva"]/ibase^2,raw[key];atol=1e-12)
    end
    pm=PM.instantiate_model(data,PM.ACPPowerModel,PowerModelsACDC.build_acdcopf;ref_extensions=REFS,setting=deepcopy(SETTINGS))
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
    physical_equations!(pm,data,vars)
    pc,pdc,ic=vars["0_pconv_ac[1]"],vars["0_pconv_dc[1]"],vars["0_iconv_ac[1]"]
    row=only([c for c in all_constraints(pm.model,QuadExpr,MOI.EqualTo{Float64})
        if coefficient(constraint_object(c).func,pc)!=0 && coefficient(constraint_object(c).func,pdc)!=0])
    obj=constraint_object(row)
    # Prove the actual assembled row uses Cinv, including its sign and scaling.
    @assert coefficient(obj.func,pc)==1 && coefficient(obj.func,pdc)==1
    @assert isapprox(coefficient(obj.func,ic,ic),-conv["LossCinv"];atol=1e-14)
    signed=(value(v->mapped[v],obj.func)-obj.set.value)*b["mva"]
    @assert isapprox(signed,case["source_loss_residual_mw"];atol=1e-10,rtol=1e-10)
    violations=primal_feasibility_report(pm.model,mapped)
    other=maximum((v for (c,v) in violations if c!=row);init=0.0)
    @assert other<1e-9
    @assert isapprox(maximum(values(violations);init=0.0),abs(case["source_loss_residual_mw"])/b["mva"];atol=1e-9)
    # Evaluate each explicitly selected branch independently in physical units.
    # Off-domain branches are witnesses, not claims about directional feasibility.
    selected=spec["selected"]=="rectifier" ? raw["LossCrec"] : raw["LossCinv"]
    delete(pm.model,row)
    branch=@constraint(pm.model,b["mva"]*(pc+pdc)==raw["LossA"]+raw["LossB"]*ibase*ic+selected*(ibase*ic)^2)
    branch_violation=maximum(values(primal_feasibility_report(pm.model,mapped));init=0.0)
    @assert branch_violation<1e-9
    sign_violation=0.0
    if get(case,"fixed_mode",false)
        if spec["selected"]=="rectifier";set_lower_bound(pc,max(lower_bound(pc),0.0));else;set_upper_bound(pc,min(upper_bound(pc),0.0));end
        signed_report=primal_feasibility_report(pm.model,mapped)
        sign_bound=spec["selected"]=="rectifier" ? LowerBoundRef(pc) : UpperBoundRef(pc)
        sign_violation=get(signed_report,sign_bound,0.0)*b["mva"]
        @assert isapprox(sign_violation,case["sign_violation_mw"];atol=1e-12,rtol=1e-12)
        @assert case["sign_violation_mw"]==0.0 || sign_violation>0.0
        @assert isapprox(maximum(values(signed_report);init=0.0),case["native_max_violation"];atol=1e-9)
    end
    @assert isapprox(value(v->mapped[v],objective_function(pm.model)),case["objective"];atol=1e-9)
    # A changed current must fail the selected loss equation even at zero current.
    corrupted=copy(mapped);corrupted[ic]+=0.01/ibase
    bad=primal_feasibility_report(pm.model,corrupted)
    @assert get(bad,branch,0.0)>0.007
    push!(records,Dict("file"=>case["file"],"source_sha256"=>bytes2hex(sha256(read(path))),
        "case"=>spec,"bases"=>b,"named_mode"=>case["named_mode"],
        "source_loss_residual_mw"=>signed,"other_reference_violation"=>other,
        "selected_branch_violation"=>branch_violation,"sign_violation_mw"=>sign_violation,"current_corruption_mw"=>bad[branch]))
end
output=Dict("records"=>records,"counts"=>Dict("physical_witnesses"=>1728,"current_corruption_controls"=>1728,"optimization_solves"=>0),
    "powerio_revision"=>"70025e87d918be195362ca63a48c9a68d2dfb702",
    "reference_revision"=>"9ada4082ac087aed649f48fb5f977680b0fe06c7",
    "reference_source_sha256"=>Dict(rel=>bytes2hex(sha256(read(joinpath(pkgdir(PowerModelsACDC),rel)))) for rel in ("src/core/constraint_template.jl","src/core/data.jl","src/formconv/acp.jl","docs/src/parser.md")),
    "julia_version"=>string(VERSION),"manifest_sha256"=>bytes2hex(sha256(read(joinpath(@__DIR__,"..","Manifest.toml")))),
    "native_report_sha256"=>bytes2hex(sha256(read(joinpath(out,"cases.json")))),
    "scope"=>all(get(c,"fixed_mode",false) for c in cases) ? "fixed_mode_pointwise_loss_and_sign_domain_validation; no_boundary_optimization_claim" : "pointwise_loss_branch_characterization; no_directional_profile_or_optimization_qualification",
    "reference_modifications"=>"restore_declared_Imax; SI_bypass_and_DC_rows; then_replace_only_loss_row_with_explicit_branch_in_physical_units; fixed_mode_cases_also_intersect_valve_P_sign_bounds")
open(joinpath(out,"reference.json"),"w") do io;JSON.print(io,output,2);println(io);end
println("1728 independent loss witnesses and 1728 current-corruption controls pass")
