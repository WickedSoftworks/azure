use std::time::Instant;

use azure_color::{plan, ChannelReport, ColorState, Environment, Fidelity, Stage};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, CLAMP_TOLERANCE,
};
use crate::mock::{MockMatrix, MockRamp};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StageLanding {
    pub stage: Stage,
    pub backend: String,
    pub ok: bool,
    pub detail: Option<String>,
}

/// What actually happened, which is the only thing the surface is allowed
/// to draw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyReport {
    pub reports: Vec<ChannelReport>,
    pub stages: Vec<StageLanding>,
    /// Wall time of the writes, microseconds. The field prints it.
    ///
    /// u32, not u64: it crosses to JavaScript as a JSON number, and an
    /// apply that took over an hour is not a thing that happens.
    pub micros: u32,
    /// True while display state differs from the panel's own defaults.
    pub dirty: bool,
}

pub struct Core {
    matrix: Box<dyn MatrixBackend>,
    ramp: Box<dyn RampBackend>,
    dirty: bool,
}

impl Core {
    pub fn new(matrix: Box<dyn MatrixBackend>, ramp: Box<dyn RampBackend>) -> Self {
        Core { matrix, ramp, dirty: false }
    }

    /// A mock-backed core, for tests and for any platform without the
    /// Windows backends.
    pub fn mock() -> Self {
        Core::new(Box::new(MockMatrix::default()), Box::new(MockRamp::default()))
    }

    pub fn displays(&self) -> Vec<DisplayInfo> {
        self.ramp.displays()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn apply(
        &mut self,
        state: &ColorState,
        env: &Environment,
        target: &LutTarget,
    ) -> ApplyReport {
        let started = Instant::now();
        let p = plan(state, env);
        let mut stages = Vec::new();
        let mut reports = p.reports;

        if let Some(m) = p.matrix {
            match self.matrix.apply(&m) {
                Ok(()) => {
                    self.dirty = true;
                    stages.push(StageLanding {
                        stage: Stage::Matrix,
                        backend: self.matrix.name().to_string(),
                        ok: true,
                        detail: None,
                    });
                }
                Err(e) => {
                    let detail = e.to_string();
                    downgrade(&mut reports, Stage::Matrix, Fidelity::Unrealised, &detail);
                    stages.push(StageLanding {
                        stage: Stage::Matrix,
                        backend: self.matrix.name().to_string(),
                        ok: false,
                        detail: Some(detail),
                    });
                }
            }
        }

        if let Some(r) = p.ramp {
            match self.ramp.apply(target, &r) {
                Ok(landings) => {
                    self.dirty = true;
                    let worst = landings.iter().map(|l| l.deviation).max().unwrap_or(0);
                    if worst > CLAMP_TOLERANCE {
                        downgrade(
                            &mut reports,
                            Stage::Lut,
                            Fidelity::Clamped,
                            &format!(
                                "readback differs by {worst} of 65535: the driver applied a narrower ramp"
                            ),
                        );
                    }
                    stages.push(StageLanding {
                        stage: Stage::Lut,
                        backend: self.ramp.name().to_string(),
                        ok: true,
                        detail: Some(format!(
                            "{} display{}",
                            landings.len(),
                            if landings.len() == 1 { "" } else { "s" }
                        )),
                    });
                }
                Err(e) => {
                    let detail = e.to_string();
                    downgrade(&mut reports, Stage::Lut, Fidelity::Unrealised, &detail);
                    stages.push(StageLanding {
                        stage: Stage::Lut,
                        backend: self.ramp.name().to_string(),
                        ok: false,
                        detail: Some(detail),
                    });
                }
            }
        }

        ApplyReport {
            reports,
            stages,
            micros: started.elapsed().as_micros().min(u32::MAX as u128) as u32,
            dirty: self.dirty,
        }
    }

    /// Puts the display back. Runs when Azure is switched off, when the
    /// engine shuts down, and — through the backends' `Drop` — when the
    /// worker thread unwinds.
    ///
    /// Recovery after a hard kill, where no Rust code runs at all, needs a
    /// dirty flag on disk and lands with preset persistence in M4. Until
    /// then a killed Azure leaves its last ramp on the display.
    pub fn restore(&mut self) -> Result<(), BackendError> {
        let a = self.matrix.clear();
        let b = self.ramp.clear();
        self.dirty = false;
        a.and(b)
    }
}

/// Rewrites the truth for every channel a failed or clamped stage was
/// carrying. A stage that did not land cannot leave `Exact` behind.
fn downgrade(reports: &mut [ChannelReport], stage: Stage, to: Fidelity, note: &str) {
    for r in reports.iter_mut().filter(|r| r.stage == Some(stage)) {
        r.fidelity = to;
        r.note = Some(note.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::{ChannelId, ColorState, Environment, Fidelity, Stage};

    #[test]
    fn a_neutral_state_writes_nothing_and_leaves_the_display_clean() {
        let mut c = Core::mock();
        let report = c.apply(&ColorState::neutral(), &Environment::ideal(), &LutTarget::All);
        assert!(report.stages.is_empty(), "neutral must not touch a backend");
        assert!(!c.is_dirty());
    }

    #[test]
    fn a_mix_change_writes_the_matrix_only() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].stage, Stage::Matrix);
        assert!(report.stages[0].ok);
        assert!(c.is_dirty());
    }

    #[test]
    fn a_gamma_change_writes_the_ramp_only() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].stage, Stage::Lut);
    }

    #[test]
    fn a_clamped_ramp_is_reported_as_clamped_not_as_success() {
        let mut c = Core::new(
            Box::new(MockMatrix::default()),
            Box::new(MockRamp::clamping(4000)),
        );
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 240);
        let env = Environment { gamma_range_unlocked: true, ..Environment::ideal() };
        let report = c.apply(&s, &env, &LutTarget::All);
        let gamma = report.reports.iter().find(|r| r.id == ChannelId::Gamma).unwrap();
        assert_eq!(gamma.fidelity, Fidelity::Clamped);
        assert!(gamma.note.as_ref().unwrap().contains("readback"));
    }

    #[test]
    fn restore_clears_both_backends_and_drops_the_dirty_flag() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert!(c.is_dirty());
        c.restore().unwrap();
        assert!(!c.is_dirty());
    }

    #[test]
    fn a_failing_backend_is_reported_and_does_not_stop_the_other_stage() {
        let mut c = Core::new(Box::new(MockMatrix::failing()), Box::new(MockRamp::default()));
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 2);
        let matrix = report.stages.iter().find(|s| s.stage == Stage::Matrix).unwrap();
        let lut = report.stages.iter().find(|s| s.stage == Stage::Lut).unwrap();
        assert!(!matrix.ok);
        assert!(lut.ok);
        let sat = report.reports.iter().find(|r| r.id == ChannelId::Saturation).unwrap();
        assert_eq!(sat.fidelity, Fidelity::Unrealised);
    }

    #[test]
    fn targeting_one_display_does_not_touch_the_others() {
        let mut ramp = MockRamp::default();
        ramp.displays = vec![
            DisplayInfo { key: "A".into(), name: "A".into(), primary: true, hdr: false },
            DisplayInfo { key: "B".into(), name: "B".into(), primary: false, hdr: false },
        ];
        let mut c = Core::new(Box::new(MockMatrix::default()), Box::new(ramp));
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::One("B".into()));
        assert_eq!(report.stages[0].detail.as_deref(), Some("1 display"));
    }
}
