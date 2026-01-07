//! # Sensor Sync
//!
//! Probabilistische Zeit-Synchronisation und Beobachtungs-Gruppierung.
//!
//! - Zeit-Offset-Modell (offset, var, drift)
//! - Gaußsche Dichte (über sensor-core)
//! - `association_probability` für Paarassoziationen
//! - Probabilistische Gruppierung ohne harte Schwellwerte

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sensor_core::likelihood as gaussian_pdf;
use sensor_redis::{GroupMember, RawObservation, SynchronizedGroup, TimeSyncState};

/// Zeit-Offset-Modell: Abbildung von lokaler Zeit auf globale Zeit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeOffsetModel {
    pub offset_mean: f64,
    pub offset_var: f64,
    pub drift: f64,
}

impl From<TimeSyncState> for TimeOffsetModel {
    fn from(value: TimeSyncState) -> Self {
        Self {
            offset_mean: value.offset_mean,
            offset_var: value.offset_var,
            drift: value.drift,
        }
    }
}

impl From<&TimeSyncState> for TimeOffsetModel {
    fn from(value: &TimeSyncState) -> Self {
        Self {
            offset_mean: value.offset_mean,
            offset_var: value.offset_var,
            drift: value.drift,
        }
    }
}

impl TimeOffsetModel {
    /// Erstelle ein neues Zeit-Offset-Modell mit Standardwerten.
    pub fn new() -> Self {
        Self {
            offset_mean: 0.0,
            offset_var: 0.1,
            drift: 1.0,
        }
    }

    /// Vorhersage der globalen Zeit aus lokaler Zeit.
    pub fn predict_global_time(&self, t_local: f64) -> f64 {
        self.offset_mean + self.drift * t_local
    }

    /// Kalman-Update mit einer neuen Beobachtung.
    /// Vereinfachtes 1D-Kalman-Filter für Offset-Schätzung.
    /// `t_local`: lokaler Zeitstempel, `t_global_measured`: gemessene globale Zeit, `measurement_var`: Messunsicherheit
    pub fn update_with_observation(&mut self, t_local: f64, t_global_measured: f64, measurement_var: f64) {
        // Predicted global time
        let t_pred = self.predict_global_time(t_local);
        // Innovation (Residuum)
        let innovation = t_global_measured - t_pred;
        // Innovation variance
        let s = self.offset_var + measurement_var;
        if s <= 0.0 {
            return; // Keine Update möglich
        }
        // Kalman gain
        let k = self.offset_var / s;
        // Update mean
        self.offset_mean += k * innovation;
        // Update variance
        self.offset_var = (1.0 - k) * self.offset_var;
        // Bound variance to avoid collapse
        self.offset_var = self.offset_var.max(1e-6);
    }
}

impl Default for TimeOffsetModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Lokale Zeit in globale Zeit abbilden: t_global = offset + drift * t_local
pub fn to_global_time(t_local: f64, model: &TimeOffsetModel) -> f64 {
    model.predict_global_time(t_local)
}

/// Effektive Varianz einer Beobachtung im globalen Zeitrahmen.
/// Einfaches additives Modell: offset_var (Clock) + sigma^2 (Messrauschen)
pub fn effective_variance(model: &TimeOffsetModel, obs_sigma: f64) -> f64 {
    // sigma ist Standardabweichung der Beobachtung; wir konvertieren zu Varianz
    let obs_var = obs_sigma * obs_sigma;
    (model.offset_var).max(0.0) + obs_var
}

/// Assoziationswahrscheinlichkeit zweier Beobachtungen basierend auf globalem
/// Zeitresiduum und kombinierter Varianz. Rückgabe ist eine Dichte, kein Bool.
pub fn association_probability(
    a: &RawObservation,
    a_model: &TimeOffsetModel,
    b: &RawObservation,
    b_model: &TimeOffsetModel,
) -> f64 {
    let ta = to_global_time(a.t_local, a_model);
    let tb = to_global_time(b.t_local, b_model);
    let dt = ta - tb;
    let var = effective_variance(a_model, a.sigma) + effective_variance(b_model, b.sigma);
    if var <= 0.0 { return 0.0; }
    gaussian_pdf(dt, var)
}

/// Schätze einen globalen Ereigniszeitpunkt als präzisionsgewichtetes Mittel.
pub fn estimate_event_time(observations: &[RawObservation], models: &[TimeOffsetModel]) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (obs, mdl) in observations.iter().zip(models.iter()) {
        let tg = to_global_time(obs.t_local, mdl);
        let var = effective_variance(mdl, obs.sigma).max(1e-12);
        let w = 1.0 / var; // Präzision
        num += w * tg;
        den += w;
    }
    if den == 0.0 { 0.0 } else { num / den }
}

/// Erzeuge eine einzige probabilistische Gruppe für einen Beobachtungsbatch.
/// Keine harten Schwellwerte: Mitgliedschaften werden aus Gauß-Dichten relativ
/// zum geschätzten Ereigniszeitpunkt normalisiert.
pub fn group_observations_probabilistically(
    observations: &[RawObservation],
    models: &[TimeOffsetModel],
) -> Result<SynchronizedGroup> {
    if observations.is_empty() { 
        return Ok(SynchronizedGroup { t_global: 0.0, members: vec![] });
    }
    assert_eq!(observations.len(), models.len(), "observations und models müssen gleich lang sein");

    // Ereigniszeitpunkt schätzen
    let t_hat = estimate_event_time(observations, models);

    // Unnormierte Mitgliedschaftsdichten berechnen
    let mut weights = Vec::with_capacity(observations.len());
    for (obs, mdl) in observations.iter().zip(models.iter()) {
        let tg = to_global_time(obs.t_local, mdl);
        let dt = tg - t_hat;
        let var = effective_variance(mdl, obs.sigma).max(1e-12);
        let w = gaussian_pdf(dt, var);
        weights.push(w);
    }

    // Normalisieren zu Wahrscheinlichkeiten
    let sum_w: f64 = weights.iter().copied().sum();
    let mut members = Vec::with_capacity(observations.len());
    for ((obs, _mdl), w) in observations.iter().zip(models.iter()).zip(weights.into_iter()) {
        let p = if sum_w > 0.0 { w / sum_w } else { 0.0 };
        members.push(GroupMember { sensor_id: obs.sensor_id.clone(), probability: p });
    }

    Ok(SynchronizedGroup { t_global: t_hat, members })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use sensor_redis::RawObservation;

    #[test]
    fn gaussian_and_association_probability() {
        let mdl_a = TimeOffsetModel { offset_mean: 0.0, offset_var: 0.01, drift: 1.0 };
        let mdl_b = TimeOffsetModel { offset_mean: 0.001, offset_var: 0.02, drift: 1.0 };
        let a = RawObservation { sensor_id: "A".into(), sensor_type: "x".into(), t_local: 1.0, sigma: 0.05, payload_ref: "mem://a".into() };
        let b = RawObservation { sensor_id: "B".into(), sensor_type: "x".into(), t_local: 1.0, sigma: 0.05, payload_ref: "mem://b".into() };
        let p = association_probability(&a, &mdl_a, &b, &mdl_b);
        assert!(p > 0.0);
        // Symmetrie
        let p2 = association_probability(&b, &mdl_b, &a, &mdl_a);
        assert_relative_eq!(p, p2, max_relative = 1e-12);
    }

    #[test]
    fn estimate_and_group_single_batch() {
        let obs = vec![
            RawObservation { sensor_id: "s1".into(), sensor_type: "cam".into(), t_local: 10.0, sigma: 0.1, payload_ref: "mem://1".into() },
            RawObservation { sensor_id: "s2".into(), sensor_type: "imu".into(), t_local: 10.05, sigma: 0.2, payload_ref: "mem://2".into() },
            RawObservation { sensor_id: "s3".into(), sensor_type: "mic".into(), t_local: 9.98, sigma: 0.15, payload_ref: "mem://3".into() },
        ];
        let models = vec![
            TimeOffsetModel { offset_mean: 0.0, offset_var: 0.01, drift: 1.0 },
            TimeOffsetModel { offset_mean: 0.0, offset_var: 0.02, drift: 1.0 },
            TimeOffsetModel { offset_mean: 0.0, offset_var: 0.015, drift: 1.0 },
        ];

        let t_hat = estimate_event_time(&obs, &models);
        assert!(t_hat > 9.9 && t_hat < 10.1);

        let group = group_observations_probabilistically(&obs, &models).unwrap();
        assert!(group.t_global > 9.9 && group.t_global < 10.1);
        assert_eq!(group.members.len(), 3);
        let sum_p: f64 = group.members.iter().map(|m| m.probability).sum();
        assert!((sum_p - 1.0).abs() < 1e-9);
    }

    #[test]
    fn kalman_update_converges() {
        let mut model = TimeOffsetModel::new();
        // Simuliere mehrere Messungen mit konstantem Offset 0.5
        for _ in 0..10 {
            model.update_with_observation(10.0, 10.5, 0.01);
        }
        // Offset sollte gegen 0.5 konvergieren
        assert!((model.offset_mean - 0.5).abs() < 0.1);
        assert!(model.offset_var < 0.1);
    }
}
