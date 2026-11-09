//! Explicit native-coordinate adapter. Stored PowerIO values are never solver keys.
use powerio::{AcDcOperatingPoint, AcDcOpfInstance, AcDcQuantity, AcDcValue};
use std::collections::BTreeMap;
pub(super) fn station_quantity(field: &str) -> Option<AcDcQuantity> {
    use AcDcQuantity::*;
    Some(match field {
        "pgrid" => PccActivePower,
        "qgrid" => PccReactivePower,
        "ptf_to" => TransformerActivePowerTo,
        "qtf_to" => TransformerReactivePowerTo,
        "ppr_fr" => ReactorActivePowerFrom,
        "qpr_fr" => ReactorReactivePowerFrom,
        "pconv" => ValveActivePower,
        "qconv" => ValveReactivePower,
        "pdc" => StationDcActivePower,
        "iconv" => ValveCurrentMagnitude,
        "vmfilt" => FilterVoltageMagnitude,
        "vafilt" => FilterVoltageAngle,
        "vmconv" => ValveVoltageMagnitude,
        "vaconv" => ValveVoltageAngle,
        _ => return None,
    })
}
fn name(quantity: AcDcQuantity, id: &str) -> Result<String, String> {
    use AcDcQuantity::*;
    Ok(match quantity {
        PccActivePower => format!("station/{id}/pgrid"),
        PccReactivePower => format!("station/{id}/qgrid"),
        TransformerActivePowerTo => format!("station/{id}/ptf_to"),
        TransformerReactivePowerTo => format!("station/{id}/qtf_to"),
        ReactorActivePowerFrom => format!("station/{id}/ppr_fr"),
        ReactorReactivePowerFrom => format!("station/{id}/qpr_fr"),
        ValveActivePower => format!("station/{id}/pconv"),
        ValveReactivePower => format!("station/{id}/qconv"),
        StationDcActivePower => format!("station/{id}/pdc"),
        ValveCurrentMagnitude => format!("station/{id}/iconv"),
        FilterVoltageMagnitude => format!("station/{id}/vmfilt"),
        FilterVoltageAngle => format!("station/{id}/vafilt"),
        ValveVoltageMagnitude => format!("station/{id}/vmconv"),
        ValveVoltageAngle => format!("station/{id}/vaconv"),
        AcVoltageMagnitude => format!("vm[{id}]"),
        AcVoltageAngle => format!("va[{id}]"),
        GeneratorActivePower => format!("pg[{id}]"),
        GeneratorReactivePower => format!("qg[{id}]"),
        AcBranchActivePowerFrom => format!("ac_branch/{id}/pf"),
        AcBranchReactivePowerFrom => format!("ac_branch/{id}/qf"),
        AcBranchActivePowerTo => format!("ac_branch/{id}/pt"),
        AcBranchReactivePowerTo => format!("ac_branch/{id}/qt"),
        DcVoltageMagnitude => format!("dc_bus/{id}/vm"),
        DcBranchActivePowerFrom => format!("dc_branch/{id}/pf"),
        DcBranchActivePowerTo => format!("dc_branch/{id}/pt"),
        _ => return Err("Unqualified PowerIO quantity".into()),
    })
}
fn scale(q: AcDcQuantity, base: f64) -> f64 {
    if matches!(q.unit(), "MW" | "MVAr") {
        base
    } else {
        1.0
    }
}
pub(super) fn encode(
    instance: &AcDcOpfInstance,
    point: &BTreeMap<String, f64>,
) -> Result<AcDcOperatingPoint, String> {
    let values = instance
        .coordinates()
        .into_iter()
        .map(|(quantity, identity)| {
            let key = name(quantity, &identity)?;
            let value = point
                .get(&key)
                .ok_or_else(|| format!("Missing native coordinate {key}"))?
                * scale(quantity, instance.network().base_mva());
            Ok(AcDcValue {
                quantity,
                identity,
                value,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if values.len() != point.len() {
        return Err("Native point contains unqualified auxiliary coordinates".into());
    }
    AcDcOperatingPoint::new(instance, values).map_err(|e| e.to_string())
}
pub(super) fn decode(
    instance: &AcDcOpfInstance,
    point: &AcDcOperatingPoint,
) -> Result<BTreeMap<String, f64>, String> {
    let checked =
        AcDcOperatingPoint::new(instance, point.values().to_vec()).map_err(|e| e.to_string())?;
    checked
        .values()
        .iter()
        .map(|v| {
            Ok((
                name(v.quantity, &v.identity)?,
                v.value / scale(v.quantity, instance.network().base_mva()),
            ))
        })
        .collect()
}
