//! # Sensor Sync
//!
//! Probabilistische Zeit-Synchronisation und Beobachtungs-Gruppierung.
//!
//! - Zeit-Offset-Modell (offset, var, drift)
//! - Gaußsche Dichte (über sensor-core)
//! - `association_probability` für Paarassoziationen
//! - Probabilistische Gruppierung ohne harte Schwellwerte

pub mod time_model;

use anyhow::Result;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use redis::Connection;
use sensor_election::write_sync_group_if_master;
use sensor_redis::{GroupMember, RawObservation, SynchronizedGroup, TimeSyncState};
pub use time_model::TimeOffset;

/// Gaußsche Wahrscheinlichkeitsdichte (PDF).
///
/// Berechnet die Wahrscheinlichkeitsdichte einer Normalverteilung N(mean, var)
/// am Punkt x. Verwendet für probabilistische Zeit-Assoziationen.
///
/// # Arguments
/// * `x` - Auswertungspunkt
/// * `mean` - Erwartungswert der Verteilung
/// * `var` - Varianz der Verteilung (nicht Standardabweichung!)
///
/// # Returns
/// Wahrscheinlichkeitsdichte an Stelle x
///
/// # Examples
/// ```
/// use sensor_sync::gaussian_pdf;
///
/// // Zentrum der Verteilung
/// let p = gaussian_pdf(0.0, 0.0, 1.0);
/// assert!((p - 0.3989422804).abs() < 1e-6);
///
/// // Symmetrie
/// let p1 = gaussian_pdf(1.0, 0.0, 1.0);
/// let p2 = gaussian_pdf(-1.0, 0.0, 1.0);
/// assert!((p1 - p2).abs() < 1e-12);
/// ```
pub fn gaussian_pdf(x: f64, mean: f64, var: f64) -> f64 {
    if var <= 0.0 {
        return 0.0;
    }
    let sigma = var.sqrt();
    let dx = x - mean;
    let exponent = -0.5 * (dx / sigma).powi(2);
    let normalization = 1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    normalization * exponent.exp()
}

/// Wahrscheinlichkeit, dass eine Beobachtung zu einem globalen Zeitpunkt gehört.
///
/// Berechnet die Gaußsche Wahrscheinlichkeitsdichte, dass eine lokale Sensor-Beobachtung
/// zum Kandidaten-Zeitpunkt t_global gehört, gegeben das Kalman-gefilterte Zeit-Offset-Modell.
///
/// # Arguments
/// * `obs` - Rohe Sensor-Beobachtung (t_local, sigma)
/// * `t_global` - Kandidat globaler Zeitpunkt
/// * `offset` - Kalman-gefiltertes Zeit-Offset-Modell (mean, variance)
///
/// # Returns
/// Wahrscheinlichkeitsdichte der Assoziation (nicht normalisiert über alle Kandidaten)
///
/// # Examples
/// ```
/// use sensor_sync::{observation_probability, TimeOffset};
/// use sensor_redis::RawObservation;
///
/// let obs = RawObservation {
///     sensor_id: "S1".into(),
///     sensor_type: "test".into(),
///     t_local: 10.0,
///     sigma: 0.1,
///     payload_ref: "mem://s1".into(),
/// };
/// let offset = TimeOffset::new();  // mean=0, variance=1.0
///
/// // Perfekter Match: t_global = t_local + offset.mean
/// let p_match = observation_probability(&obs, 10.0, &offset);
///
/// // Abweichung: t_global weiter weg
/// let p_far = observation_probability(&obs, 15.0, &offset);
///
/// assert!(p_match > p_far);
/// ```
pub fn observation_probability(obs: &RawObservation, t_global: f64, offset: &TimeOffset) -> f64 {
    // Paper Section 4.3: Likelihood-based association
    // Erwartete globale Zeit: t_local + offset
    let t_expected = obs.t_local + offset.offset_mean;
    let dt = t_global - t_expected;
    
    // Gesamtvarianz: Kalman-Unsicherheit + Messrauschen
    let var = offset.offset_variance + obs.sigma.powi(2);
    
    gaussian_pdf(dt, 0.0, var)
}

/// Bestimme Kandidaten-Buckets für Zeitpunkt-Suche in Redis.
///
/// Gegeben ein globaler Zeitpunkt und ein Offset-Modell, berechne die Redis-Bucket-IDs,
/// in denen relevante Beobachtungen gespeichert sein könnten. Berücksichtigt Unsicherheit
/// durch ±1 Bucket-Nachbarschaft.
///
/// # Arguments
/// * `t_global` - Globaler Zeitpunkt (Sekunden)
/// * `offset` - Kalman-gefiltertes Zeit-Offset-Modell
/// * `bucket_size_ms` - Redis-Bucket-Größe in Millisekunden
///
/// # Returns
/// Vektor von Bucket-IDs (sortiert, dedupliziert)
///
/// # Examples
/// ```
/// use sensor_sync::{candidate_buckets, TimeOffset};
///
/// let offset = TimeOffset::new();
/// let buckets = candidate_buckets(10.0, &offset, 1000);
/// 
/// // Für t_global=10.0s, offset_mean=0: t_local ≈ 10.0s → Bucket 10
/// // Mit ±1 Nachbarschaft: [9, 10, 11]
/// assert_eq!(buckets.len(), 3);
/// assert!(buckets.contains(&10));
/// ```
pub fn candidate_buckets(t_global: f64, offset: &TimeOffset, bucket_size_ms: u64) -> Vec<u64> {
    // Invertierte Abbildung: t_local ≈ (t_global - offset_mean) / drift
    let t_local_expected = (t_global - offset.offset_mean) / offset.drift;
    
    // Konvertiere zu Millisekunden und berechne Zentral-Bucket
    let t_local_ms = (t_local_expected * 1000.0) as i64;
    let bucket_size = bucket_size_ms as i64;
    let center_bucket = t_local_ms / bucket_size;
    
    // ±1 Bucket-Nachbarschaft für Unsicherheit
    let mut buckets = vec![
        (center_bucket - 1).max(0) as u64,
        center_bucket.max(0) as u64,
        (center_bucket + 1).max(0) as u64,
    ];
    
    // Deduplizierung (falls center_bucket=0 → bucket-1 auch 0)
    buckets.sort_unstable();
    buckets.dedup();
    
    buckets
}

/// Bucket-ID einer Beobachtung berechnen (Millisekunden-Auflösung).
pub fn observation_bucket_id(t_local: f64, bucket_size_ms: u64) -> u64 {
    if t_local <= 0.0 {
        return 0;
    }
    let t_ms = (t_local * 1000.0) as i64;
    let bucket = t_ms / bucket_size_ms as i64;
    if bucket < 0 { 0 } else { bucket as u64 }
}

/// Gruppiere Beobachtungen probabilistisch in ein Time Slice für einen gegebenen t_global.
///
/// - Kandidaten werden per Bucket-Nachbarschaft (±1) gefiltert.
/// - Mitgliedschaften sind gaußsche Dichten relativ zu `t_global` (kein harter Schwellwert).
/// - Gewichte werden normalisiert, sodass `sum(probability)=1` für die ausgewählten Mitglieder.
pub fn group_time_slice_probabilistically(
    t_global: f64,
    observations: &[RawObservation],
    offsets: &HashMap<String, TimeOffset>,
    bucket_size_ms: u64,
) -> SynchronizedGroup {
    let mut members = Vec::new();
    let mut weights = Vec::new();

    for obs in observations {
        // Offset-Lookup; unbekannte Sensoren überspringen
        let Some(offset) = offsets.get(&obs.sensor_id) else { continue };

        // Bucket-Filter mit ±1 Nachbarschaft
        let obs_bucket = observation_bucket_id(obs.t_local, bucket_size_ms);
        let candidates = candidate_buckets(t_global, offset, bucket_size_ms);
        if !candidates.contains(&obs_bucket) {
            continue;
        }

        // Gewicht = Gauß-Dichte der Zeitabweichung
        let w = observation_probability(obs, t_global, offset);
        members.push(GroupMember {
            sensor_id: obs.sensor_id.clone(),
            probability: 0.0, // Platzhalter bis Normalisierung
        });
        weights.push(w);
    }

    // Normalisieren
    let sum_w: f64 = weights.iter().copied().sum();
    if sum_w > 0.0 {
        for (m, w) in members.iter_mut().zip(weights.into_iter()) {
            m.probability = w / sum_w;
        }
    } else {
        for m in members.iter_mut() {
            m.probability = 0.0;
        }
    }

    SynchronizedGroup { t_global, members }
}

/// Standardisierte Gruppen-ID aus globalem Zeitpunkt (Nanosekunden gerundet).
pub fn time_slice_group_id(t_global: f64) -> String {
    // Defensive: NaN/inf → 0
    if !t_global.is_finite() {
        return "g:0".into();
    }
    let ns = (t_global * 1e9).round() as i128;
    format!("g:{ns}")
}

/// Schreibe einen Time Slice nur, wenn der aufrufende Node Master ist.
pub fn persist_time_slice_if_master(
    con: &mut Connection,
    node_id: &str,
    group: &SynchronizedGroup,
) -> Result<()> {
    let group_id = time_slice_group_id(group.t_global);
    write_sync_group_if_master(con, node_id, &group_id, group)
}

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
    gaussian_pdf(dt, 0.0, var)
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
        let w = gaussian_pdf(dt, 0.0, var);
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
    use std::collections::HashMap;

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

    #[test]
    fn gaussian_pdf_symmetry() {
        let mean = 5.0;
        let var = 2.0;
        
        // Test Symmetrie um mean
        let p_left = gaussian_pdf(mean - 1.0, mean, var);
        let p_right = gaussian_pdf(mean + 1.0, mean, var);
        assert!((p_left - p_right).abs() < 1e-12);
        
        // Test mit verschiedenen Abständen
        let p_left2 = gaussian_pdf(mean - 0.5, mean, var);
        let p_right2 = gaussian_pdf(mean + 0.5, mean, var);
        assert!((p_left2 - p_right2).abs() < 1e-12);
    }

    #[test]
    fn gaussian_pdf_normalization() {
        let mean = 0.0;
        let var = 1.0;
        
        // Zentrum der Standardnormalverteilung
        let p_center = gaussian_pdf(mean, mean, var);
        let expected = 1.0 / (2.0 * std::f64::consts::PI).sqrt();
        assert!((p_center - expected).abs() < 1e-9);
        
        // Test: PDF am Zentrum ist maximal
        let p_offset = gaussian_pdf(mean + 1.0, mean, var);
        assert!(p_center > p_offset);
        
        // Numerische Integration (Trapezregel) sollte ≈1 ergeben
        let mut integral = 0.0;
        let dx = 0.01;
        let range = 5.0; // ±5σ
        let mut x = mean - range;
        while x <= mean + range {
            integral += gaussian_pdf(x, mean, var) * dx;
            x += dx;
        }
        // Integral sollte nahe 1.0 sein (nicht exakt wegen endlicher Range)
        assert!((integral - 1.0).abs() < 0.01);
    }

    #[test]
    fn gaussian_pdf_nonpositive_variance() {
        assert_eq!(gaussian_pdf(0.0, 0.0, 0.0), 0.0);
        assert_eq!(gaussian_pdf(1.0, 0.0, -1.0), 0.0);
    }

    #[test]
    fn gaussian_pdf_different_variances() {
        let mean = 0.0;
        let x = 1.0;
        
        // Höhere Varianz → niedrigere Dichte am Peak, breitere Verteilung
        let p_narrow = gaussian_pdf(x, mean, 0.1);
        let p_wide = gaussian_pdf(x, mean, 10.0);
        
        // Bei x=1.0 und mean=0.0: schmale Verteilung hat niedrigere Dichte
        // (da 1.0 weiter vom Zentrum in σ-Einheiten)
        assert!(p_wide > p_narrow);
    }

    #[test]
    fn observation_probability_maximal_at_perfect_match() {
        let obs = RawObservation {
            sensor_id: "S1".into(),
            sensor_type: "test".into(),
            t_local: 10.0,
            sigma: 0.1,
            payload_ref: "test:42".into(),
        };
        let offset = TimeOffset::new(); // offset_mean=0, offset_variance=0.1
        
        // Perfekter Match: t_global = t_local + offset.offset_mean
        let t_perfect = obs.t_local + offset.offset_mean;
        let p_perfect = observation_probability(&obs, t_perfect, &offset);
        
        // Abweichungen sollten niedrigere Wahrscheinlichkeit haben
        let p_plus = observation_probability(&obs, t_perfect + 0.5, &offset);
        let p_minus = observation_probability(&obs, t_perfect - 0.5, &offset);
        
        assert!(p_perfect > p_plus);
        assert!(p_perfect > p_minus);
        // Symmetrie
        assert!((p_plus - p_minus).abs() < 1e-12);
    }

    #[test]
    fn observation_probability_falls_with_distance() {
        let obs = RawObservation {
            sensor_id: "S2".into(),
            sensor_type: "test".into(),
            t_local: 5.0,
            sigma: 0.05,
            payload_ref: "test:123".into(),
        };
        let mut offset = TimeOffset::new();
        offset.offset_mean = 2.0; // t_expected = 5.0 + 2.0 = 7.0
        
        let t_expected = obs.t_local + offset.offset_mean;
        
        // Test mit zunehmenden Abständen
        let p0 = observation_probability(&obs, t_expected, &offset);
        let p1 = observation_probability(&obs, t_expected + 1.0, &offset);
        let p2 = observation_probability(&obs, t_expected + 2.0, &offset);
        let p3 = observation_probability(&obs, t_expected + 3.0, &offset);
        
        assert!(p0 > p1);
        assert!(p1 > p2);
        assert!(p2 > p3);
    }

    #[test]
    fn observation_probability_incorporates_uncertainties() {
        let obs = RawObservation {
            sensor_id: "Test".into(),
            sensor_type: "test".into(),
            t_local: 10.0,
            sigma: 0.1,
            payload_ref: "test:1".into(),
        };
        
        let offset_low_var = TimeOffset::with_values(0.0, 0.01, 1.0);
        let offset_high_var = TimeOffset::with_values(0.0, 1.0, 1.0);
        
        let t_global = 10.0; // Perfekter Match
        
        // Niedrige Offset-Varianz: schmalere Verteilung, höhere Spitze
        let p_low = observation_probability(&obs, t_global, &offset_low_var);
        
        // Hohe Offset-Varianz: breitere Verteilung, niedrigere Spitze
        let p_high = observation_probability(&obs, t_global, &offset_high_var);
        
        // Am Peak (dt=0) hat schmale Verteilung höhere Dichte
        assert!(p_low > p_high);
    }

    #[test]
    fn observation_bucket_id_basic() {
        assert_eq!(observation_bucket_id(1.234, 1000), 1); // 1234ms → Bucket 1
        assert_eq!(observation_bucket_id(0.001, 1000), 0); // 1ms → Bucket 0
        assert_eq!(observation_bucket_id(-1.0, 1000), 0); // negative Zeit → 0
    }

    #[test]
    fn time_slice_group_id_rounds_ns() {
        let gid = time_slice_group_id(1.234_567_89);
        assert_eq!(gid, "g:1234567890"); // 1.23456789s → 1_234_567_890ns gerundet

        let gid_nan = time_slice_group_id(f64::NAN);
        assert_eq!(gid_nan, "g:0");
    }

    #[test]
    fn candidate_buckets_returns_center_and_neighbors() {
        let offset = TimeOffset::new(); // offset_mean=0, drift=1.0

        // t_global=10.0s → t_local≈10.0s → 10000ms → Bucket 10 (bei 1000ms Buckets)
        let buckets = candidate_buckets(10.0, &offset, 1000);
        assert_eq!(buckets, vec![9, 10, 11]);

        // t_global=5.5s → t_local≈5.5s → 5500ms → Bucket 5
        let buckets = candidate_buckets(5.5, &offset, 1000);
        assert_eq!(buckets, vec![4, 5, 6]);
    }

    #[test]
    fn candidate_buckets_with_offset() {
        let mut offset = TimeOffset::new();
        offset.offset_mean = 2.0; // t_local = (t_global - 2.0) / 1.0

        // t_global=12.0s → t_local≈10.0s → Bucket 10
        let buckets = candidate_buckets(12.0, &offset, 1000);
        assert_eq!(buckets, vec![9, 10, 11]);
    }

    #[test]
    fn candidate_buckets_with_drift() {
        let mut offset = TimeOffset::new();
        offset.drift = 2.0; // t_local = t_global / 2.0

        // t_global=20.0s → t_local=10.0s → Bucket 10
        let buckets = candidate_buckets(20.0, &offset, 1000);
        assert_eq!(buckets, vec![9, 10, 11]);
    }

    #[test]
    fn candidate_buckets_handles_zero_boundary() {
        let offset = TimeOffset::new();

        // t_global=0.5s → t_local≈0.5s → Bucket 0
        // ±1 Nachbarschaft: [-1, 0, 1] → max(0) → [0, 0, 1] → dedup → [0, 1]
        let buckets = candidate_buckets(0.5, &offset, 1000);
        assert_eq!(buckets, vec![0, 1]);
    }

    #[test]
    fn candidate_buckets_different_bucket_sizes() {
        let offset = TimeOffset::new();

        // bucket_size=500ms: t_global=10.0s → Bucket 20
        let buckets = candidate_buckets(10.0, &offset, 500);
        assert_eq!(buckets, vec![19, 20, 21]);

        // bucket_size=2000ms: t_global=10.0s → Bucket 5
        let buckets = candidate_buckets(10.0, &offset, 2000);
        assert_eq!(buckets, vec![4, 5, 6]);
    }

    #[test]
    fn group_time_slice_filters_by_bucket_and_normalizes() {
        // Beobachtungen in verschiedenen Buckets
        let obs = vec![
            RawObservation { sensor_id: "s1".into(), sensor_type: "cam".into(), t_local: 10.0, sigma: 0.1, payload_ref: "mem://1".into() },
            RawObservation { sensor_id: "s2".into(), sensor_type: "cam".into(), t_local: 12.5, sigma: 0.1, payload_ref: "mem://2".into() }, // anderer Bucket
        ];

        let mut offsets = HashMap::new();
        offsets.insert("s1".into(), TimeOffset::new());
        offsets.insert("s2".into(), TimeOffset::new());

        let group = group_time_slice_probabilistically(10.0, &obs, &offsets, 1000);
        assert_eq!(group.members.len(), 1); // s2 gefiltert
        assert_eq!(group.members[0].sensor_id, "s1");
        assert!((group.members[0].probability - 1.0).abs() < 1e-9);
    }

    #[test]
    fn group_time_slice_weights_and_normalizes() {
        let obs = vec![
            RawObservation { sensor_id: "a".into(), sensor_type: "x".into(), t_local: 10.0, sigma: 0.1, payload_ref: "mem://a".into() },
            RawObservation { sensor_id: "b".into(), sensor_type: "x".into(), t_local: 10.2, sigma: 0.1, payload_ref: "mem://b".into() },
        ];
        let mut offsets = HashMap::new();
        offsets.insert("a".into(), TimeOffset::new());
        offsets.insert("b".into(), TimeOffset::new());

        let group = group_time_slice_probabilistically(10.0, &obs, &offsets, 1000);
        assert_eq!(group.members.len(), 2);

        // Wahrscheinlichkeiten müssen zu 1 normalisiert sein
        let sum_p: f64 = group.members.iter().map(|m| m.probability).sum();
        assert!((sum_p - 1.0).abs() < 1e-9);

        // s1 (10.0) näher am t_global als s2 (10.2) → höhere probability
        let p_a = group.members.iter().find(|m| m.sensor_id == "a").unwrap().probability;
        let p_b = group.members.iter().find(|m| m.sensor_id == "b").unwrap().probability;
        assert!(p_a > p_b);
    }

    #[test]
    fn group_time_slice_handles_empty_or_unknown() {
        let obs: Vec<RawObservation> = vec![];
        let offsets: HashMap<String, TimeOffset> = HashMap::new();

        let group = group_time_slice_probabilistically(5.0, &obs, &offsets, 1000);
        assert_eq!(group.t_global, 5.0);
        assert!(group.members.is_empty());

        // Unbekannter Sensor wird ignoriert
        let obs = vec![RawObservation { sensor_id: "unknown".into(), sensor_type: "x".into(), t_local: 1.0, sigma: 0.1, payload_ref: "mem://x".into() }];
        let group = group_time_slice_probabilistically(1.0, &obs, &offsets, 1000);
        assert!(group.members.is_empty());
    }
}
