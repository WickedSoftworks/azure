use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::channel::{ChannelId, ChannelRange, ColorState};
use crate::matrix::Mat5;
use crate::ops::{AffineOp, MixOp, OpGroup, PowerOp};
use crate::ramp::Ramp;

/// Gamma factors x100 that survive the GDI range clamp when
/// `GdiIcmGammaRange` has not been set to 256. Conservative: the runtime
/// readback in `azure-display` is what decides the truth for a given write.
pub const GAMMA_LOCKED_REACHABLE: [i32; 2] = [70, 140];

/// What the machine can do right now. Every field is measured, never
/// assumed; `azure-display::probe` fills it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Environment {
    /// The Magnification API initialised and nothing else owns the slot.
    pub matrix_available: bool,
    /// At least one display accepted a gamma ramp handle.
    pub lut_available: bool,
    /// A game is bypassing the compositor, so the matrix reaches nothing.
    pub exclusive_fullscreen: bool,
    /// Any targeted display is in HDR mode.
    pub hdr_active: bool,
    /// HKLM ICM GdiIcmGammaRange == 256.
    pub gamma_range_unlocked: bool,
    /// Windows Colour Filters owns the fullscreen colour effect. We yield.
    pub color_filters_active: bool,
}

impl Environment {
    /// An SDR desktop with both stages working and the gamma range still
    /// locked — the default Windows machine.
    pub fn ideal() -> Self {
        Environment {
            matrix_available: true,
            lut_available: true,
            exclusive_fullscreen: false,
            hdr_active: false,
            gamma_range_unlocked: false,
            color_filters_active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Stage {
    Matrix,
    Lut,
}

/// How faithfully the routed stage can express a channel. Not a boolean:
/// vibrance is genuinely approximate on both vendor-neutral backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Fidelity {
    Exact,
    Approximate,
    Clamped,
    Inert,
    Unrealised,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Routing {
    pub mix: Option<Stage>,
    pub affine: Option<Stage>,
    pub power: Option<Stage>,
}

/// Routes whole op-groups, never channels.
///
/// The LUT is preferred for everything it can carry because it survives
/// process exit and keeps working in exclusive fullscreen. HDR takes the LUT
/// out entirely — Windows ignores the ramp — so the affine group moves to
/// the matrix as one piece rather than being split.
pub fn route(env: &Environment) -> Routing {
    let matrix_usable = env.matrix_available && !env.color_filters_active;
    let lut_usable = env.lut_available && !env.hdr_active;

    Routing {
        mix: matrix_usable.then_some(Stage::Matrix),
        affine: if lut_usable {
            Some(Stage::Lut)
        } else if matrix_usable {
            Some(Stage::Matrix)
        } else {
            None
        },
        power: lut_usable.then_some(Stage::Lut),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChannelReport {
    pub id: ChannelId,
    pub key: String,
    pub name: String,
    pub range: ChannelRange,
    pub stage: Option<Stage>,
    pub fidelity: Fidelity,
    /// The sub-range that actually reaches the panel when something outside
    /// Azure is limiting it. `None` means the whole range is reachable.
    pub reachable: Option<[i32; 2]>,
    /// Present whenever fidelity is not `Exact`. States the machine fact.
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ApplyPlan {
    pub matrix: Option<Mat5>,
    pub ramp: Option<Ramp>,
    pub reports: Vec<ChannelReport>,
}

const AFFINE_UNIT: AffineOp = AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.0, 1.0, 1.0] };
const POWER_FLAT: PowerOp = PowerOp { gamma: 1.0 };

/// Builds the payloads and the per-channel truth for one state in one
/// environment. Pure — the backends do the writing, this decides what.
pub fn plan(state: &ColorState, env: &Environment) -> ApplyPlan {
    let routing = route(env);
    let mix = MixOp::from_state(state);
    let affine = AffineOp::from_state(state);
    let power = PowerOp::from_state(state);

    let mut matrix = Mat5::IDENTITY;
    let mut wrote_matrix = false;
    if routing.mix == Some(Stage::Matrix) && !mix.is_identity() {
        matrix = matrix.mul(Mat5::from_mix(&mix));
        wrote_matrix = true;
    }
    if routing.affine == Some(Stage::Matrix) && !affine.is_identity() {
        matrix = matrix.mul(Mat5::affine(&affine));
        wrote_matrix = true;
    }

    let lut_affine = routing.affine == Some(Stage::Lut) && !affine.is_identity();
    let lut_power = routing.power == Some(Stage::Lut) && !power.is_identity();
    let ramp = (lut_affine || lut_power).then(|| {
        Ramp::build(
            if lut_affine { &affine } else { &AFFINE_UNIT },
            if lut_power { &power } else { &POWER_FLAT },
        )
    });

    ApplyPlan {
        matrix: wrote_matrix.then_some(matrix),
        ramp,
        reports: ChannelId::ALL.iter().map(|id| report(*id, &routing, env)).collect(),
    }
}

fn report(id: ChannelId, routing: &Routing, env: &Environment) -> ChannelReport {
    let stage = match id.group() {
        OpGroup::Mix => routing.mix,
        OpGroup::Affine => routing.affine,
        OpGroup::Power => routing.power,
    };

    let (fidelity, reachable, note): (Fidelity, Option<[i32; 2]>, Option<&str>) = match (stage, id) {
        (None, ChannelId::Gamma) => (
            Fidelity::Unrealised,
            None,
            Some("no vendor-neutral path: the compositor matrix is affine and cannot express a power curve"),
        ),
        (None, _) if env.color_filters_active => (
            Fidelity::Unrealised,
            None,
            Some("Windows Colour Filters owns the fullscreen colour effect; Azure yields to it"),
        ),
        (None, _) => (
            Fidelity::Unrealised,
            None,
            Some("no backend on this machine can carry this channel"),
        ),
        (Some(Stage::Matrix), _) if env.exclusive_fullscreen => (
            Fidelity::Inert,
            None,
            Some("a game is bypassing the compositor; the matrix reaches nothing until it exits"),
        ),
        (Some(Stage::Matrix), ChannelId::Vibrance) => (
            Fidelity::Approximate,
            None,
            Some("realised as a flat saturation gain: a 5x5 affine matrix cannot express saturation-dependent gain"),
        ),
        (Some(Stage::Matrix), _) if id.group() == OpGroup::Affine => (
            Fidelity::Approximate,
            None,
            Some("routed to the compositor matrix because the scanout LUT cannot carry it here; lost on exit and in exclusive fullscreen"),
        ),
        (Some(Stage::Matrix), _) if env.hdr_active => (
            Fidelity::Approximate,
            None,
            Some("HDR: the compositor blends in scRGB, so the matrix lands differently than it does in SDR"),
        ),
        (Some(Stage::Matrix), _) => (Fidelity::Exact, None, None),
        (Some(Stage::Lut), ChannelId::Gamma) if !env.gamma_range_unlocked => (
            Fidelity::Clamped,
            Some(GAMMA_LOCKED_REACHABLE),
            Some("Windows is clamping this range until GdiIcmGammaRange is unlocked"),
        ),
        (Some(Stage::Lut), _) => (Fidelity::Exact, None, None),
    };

    ChannelReport {
        id,
        key: id.key().to_string(),
        name: id.name().to_string(),
        range: id.range(),
        stage,
        fidelity,
        reachable,
        note: note.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ChannelId::*;

    fn fidelity_of(plan: &ApplyPlan, id: crate::ChannelId) -> Fidelity {
        plan.reports.iter().find(|r| r.id == id).unwrap().fidelity
    }

    fn stage_of(plan: &ApplyPlan, id: crate::ChannelId) -> Option<Stage> {
        plan.reports.iter().find(|r| r.id == id).unwrap().stage
    }

    #[test]
    fn ideal_desktop_routes_mix_to_matrix_and_the_rest_to_the_lut() {
        let r = route(&Environment::ideal());
        assert_eq!(r.mix, Some(Stage::Matrix));
        assert_eq!(r.affine, Some(Stage::Lut));
        assert_eq!(r.power, Some(Stage::Lut));
    }

    #[test]
    fn five_of_eight_channels_sit_in_the_lut_on_an_ideal_desktop() {
        // The claim the product is built on. If this number moves, the
        // README and the surface are both wrong.
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        let in_lut = p.reports.iter().filter(|r| r.stage == Some(Stage::Lut)).count();
        assert_eq!(in_lut, 5);
    }

    #[test]
    fn the_affine_group_never_splits_across_backends() {
        let envs = [
            Environment::ideal(),
            Environment { hdr_active: true, ..Environment::ideal() },
            Environment { lut_available: false, ..Environment::ideal() },
            Environment { matrix_available: false, ..Environment::ideal() },
            Environment { exclusive_fullscreen: true, ..Environment::ideal() },
        ];
        for env in envs {
            let p = plan(&ColorState::neutral(), &env);
            let stages: std::collections::HashSet<_> = [Brightness, Contrast, Temperature, Tint]
                .iter()
                .map(|id| stage_of(&p, *id))
                .collect();
            assert_eq!(stages.len(), 1, "affine group split in {env:?}");
        }
    }

    #[test]
    fn affine_falls_back_to_the_matrix_when_the_lut_cannot_carry_it() {
        let env = Environment { lut_available: false, ..Environment::ideal() };
        assert_eq!(route(&env).affine, Some(Stage::Matrix));
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(fidelity_of(&p, Brightness), Fidelity::Approximate);
    }

    #[test]
    fn gamma_has_no_path_without_the_lut_because_the_matrix_is_affine() {
        let env = Environment { lut_available: false, ..Environment::ideal() };
        assert_eq!(route(&env).power, None);
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(fidelity_of(&p, Gamma), Fidelity::Unrealised);
        assert_eq!(stage_of(&p, Gamma), None);
    }

    #[test]
    fn vibrance_is_always_approximate() {
        for env in [
            Environment::ideal(),
            Environment { hdr_active: true, ..Environment::ideal() },
        ] {
            let p = plan(&ColorState::neutral(), &env);
            assert_eq!(fidelity_of(&p, Vibrance), Fidelity::Approximate);
            assert!(p.reports.iter().find(|r| r.id == Vibrance).unwrap().note.is_some());
        }
    }

    #[test]
    fn exclusive_fullscreen_makes_the_matrix_channels_inert() {
        let env = Environment { exclusive_fullscreen: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        for id in [Vibrance, Saturation, Hue] {
            assert_eq!(fidelity_of(&p, id), Fidelity::Inert, "{id:?}");
        }
        for id in [Brightness, Contrast, Gamma, Temperature, Tint] {
            assert_ne!(fidelity_of(&p, id), Fidelity::Inert, "{id:?} should survive");
        }
    }

    #[test]
    fn hdr_pushes_affine_to_the_matrix_and_leaves_gamma_with_no_path() {
        let env = Environment { hdr_active: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(stage_of(&p, Brightness), Some(Stage::Matrix));
        assert_eq!(fidelity_of(&p, Gamma), Fidelity::Unrealised);
        assert_eq!(fidelity_of(&p, Saturation), Fidelity::Approximate);
    }

    #[test]
    fn colour_filters_take_the_matrix_slot_and_we_yield() {
        let env = Environment { color_filters_active: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        for id in [Vibrance, Saturation, Hue] {
            assert_eq!(fidelity_of(&p, id), Fidelity::Unrealised, "{id:?}");
        }
        assert!(p.matrix.is_none());
        // The LUT half is untouched by the accessibility setting.
        assert_eq!(stage_of(&p, Gamma), Some(Stage::Lut));
    }

    #[test]
    fn locked_gamma_range_reports_clamped_with_the_reachable_window() {
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        let g = p.reports.iter().find(|r| r.id == Gamma).unwrap();
        assert_eq!(g.fidelity, Fidelity::Clamped);
        assert_eq!(g.reachable, Some(GAMMA_LOCKED_REACHABLE));
    }

    #[test]
    fn unlocking_the_gamma_range_clears_the_clamp() {
        let env = Environment { gamma_range_unlocked: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        let g = p.reports.iter().find(|r| r.id == Gamma).unwrap();
        assert_eq!(g.fidelity, Fidelity::Exact);
        assert_eq!(g.reachable, None);
    }

    #[test]
    fn a_neutral_state_builds_no_payload_at_all() {
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        assert!(p.matrix.is_none(), "neutral must not write a matrix");
        assert!(p.ramp.is_none(), "neutral must not write a ramp");
    }

    #[test]
    fn a_moved_channel_builds_only_the_payload_that_carries_it() {
        let mut s = ColorState::neutral();
        s.set(Vibrance, 156);
        let p = plan(&s, &Environment::ideal());
        assert!(p.matrix.is_some());
        assert!(p.ramp.is_none());

        let mut s = ColorState::neutral();
        s.set(Gamma, 112);
        let p = plan(&s, &Environment::ideal());
        assert!(p.matrix.is_none());
        assert!(p.ramp.is_some());
    }

    #[test]
    fn affine_on_the_matrix_carries_the_gain_but_not_the_gamma() {
        let mut s = ColorState::neutral();
        s.set(Brightness, 20);
        s.set(Gamma, 150);
        let env = Environment { lut_available: false, ..Environment::ideal() };
        let p = plan(&s, &env);
        let m = p.matrix.expect("affine should have fallen back to the matrix");
        assert!(!m.is_identity());
        assert!(p.ramp.is_none());
    }
}
