//! # Time Model
//!
//! Kalman-basiertes Zeitoffset-Modell für Sensor-zu-Referenz-Zeitabbildung.
//! Implementiert prädiktive und korrigierende Schritte für probabilistische
//! Zeit-Synchronisation ohne Clock-Synchronisation.

use serde::{Deserialize, Serialize};

/// Zeitoffset-Modell mit Kalman-Filter-Zustand.
///
/// Modelliert die Abbildung: t_global = offset_mean + drift * t_local
/// mit Gaußscher Unsicherheit über offset_mean.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeOffset {
    /// Erwartungswert des Zeitoffsets (Sekunden)
    pub offset_mean: f64,
    /// Varianz des Zeitoffsets (Sekunden²)
    pub offset_variance: f64,
    /// Drift-Faktor (dimensionslos, typischerweise ~1.0)
    pub drift: f64,
}

impl TimeOffset {
    /// Erstelle neues TimeOffset-Modell mit Standardwerten.
    ///
    /// # Returns
    /// TimeOffset mit offset_mean=0, offset_variance=0.1, drift=1.0
    pub fn new() -> Self {
        Self {
            offset_mean: 0.0,
            offset_variance: 0.1,
            drift: 1.0,
        }
    }

    /// Erstelle TimeOffset mit spezifischen Werten.
    pub fn with_values(offset_mean: f64, offset_variance: f64, drift: f64) -> Self {
        Self {
            offset_mean,
            offset_variance,
            drift,
        }
    }

    /// Kalman-Prädiktion: propagiere Unsicherheit über Zeit.
    ///
    /// Erhöht offset_variance um process_noise * dt².
    /// Verwendet für zeit-korreliertes Rauschen in der Systemdynamik.
    ///
    /// # Arguments
    /// * `dt` - Zeitdifferenz seit letztem Update (Sekunden)
    /// * `process_noise` - Prozessrauschen-Intensität (Sekunden²/Sekunde)
    pub fn predict(&mut self, dt: f64, process_noise: f64) {
        // Zustandsprädiktion: offset_mean bleibt gleich (konstantes Offset-Modell)
        // Kovarianzprädiktion: P_k|k-1 = P_k-1 + Q
        self.offset_variance += process_noise * dt.abs();
        // Begrenze Varianz nach oben (numerische Stabilität)
        self.offset_variance = self.offset_variance.min(10.0);
    }

    /// Berechne globale Zeit aus lokaler Zeit.
    ///
    /// # Arguments
    /// * `t_local` - Lokaler Zeitstempel (Sekunden)
    ///
    /// # Returns
    /// Geschätzte globale Zeit (Sekunden)
    pub fn predict_global_time(&self, t_local: f64) -> f64 {
        self.offset_mean + self.drift * t_local
    }

    /// Kalman-Update mit einer neuen Zeitmessung.
    ///
    /// Fusioniert Vorhersage mit Messung via optimaler Kalman-Gain.
    ///
    /// # Arguments
    /// * `measurement` - Gemessene globale Zeit (Sekunden)
    /// * `measurement_variance` - Messunsicherheit (Sekunden²)
    /// * `t_local` - Lokaler Zeitstempel der Messung (Sekunden)
    pub fn kalman_update(&mut self, measurement: f64, measurement_variance: f64, t_local: f64) {
        // Messung vorhersagen
        let predicted = self.predict_global_time(t_local);
        
        // Innovation (Residuum)
        let innovation = measurement - predicted;
        
        // Innovations-Kovarianz: S = H P H^T + R
        // Bei linearem Messmodell H=1: S = P + R
        let innovation_variance = self.offset_variance + measurement_variance;
        
        // Kalman-Gain: K = P H^T S^-1
        if innovation_variance <= 0.0 {
            // Keine Information: Skip Update
            return;
        }
        let kalman_gain = self.offset_variance / innovation_variance;
        
        // Zustandsupdate: x = x + K * innovation
        self.offset_mean += kalman_gain * innovation;
        
        // Kovarianzupdate: P = (I - K H) P = (1 - K) P
        self.offset_variance = (1.0 - kalman_gain) * self.offset_variance;
        
        // Begrenze Varianz nach unten (numerische Stabilität, verhindere Überkonfidenz)
        self.offset_variance = self.offset_variance.max(1e-6);
    }
}

impl Default for TimeOffset {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default_values() {
        let offset = TimeOffset::new();
        assert_eq!(offset.offset_mean, 0.0);
        assert_eq!(offset.offset_variance, 0.1);
        assert_eq!(offset.drift, 1.0);
    }

    #[test]
    fn predict_global_time_linear_model() {
        let offset = TimeOffset::with_values(0.5, 0.01, 1.0);
        let t_global = offset.predict_global_time(10.0);
        assert!((t_global - 10.5).abs() < 1e-9);
    }

    #[test]
    fn predict_global_time_with_drift() {
        let offset = TimeOffset::with_values(0.0, 0.01, 1.0001);
        let t_global = offset.predict_global_time(1000.0);
        // 0.0 + 1.0001 * 1000.0 = 1000.1
        assert!((t_global - 1000.1).abs() < 1e-9);
    }

    #[test]
    fn predict_increases_variance() {
        let mut offset = TimeOffset::with_values(0.0, 0.01, 1.0);
        let initial_var = offset.offset_variance;
        offset.predict(1.0, 0.001);
        assert!(offset.offset_variance > initial_var);
        assert!((offset.offset_variance - 0.011).abs() < 1e-9);
    }

    #[test]
    fn kalman_update_converges_to_true_offset() {
        let mut offset = TimeOffset::new();
        let true_offset = 0.5;
        let measurement_var = 0.01;
        
        // Simuliere 10 Messungen mit wahrem Offset 0.5
        for i in 1..=10 {
            let t_local = i as f64;
            let measurement = true_offset + t_local; // true_global = true_offset + 1.0 * t_local
            offset.kalman_update(measurement, measurement_var, t_local);
        }
        
        // Offset sollte gegen wahren Wert konvergieren
        assert!((offset.offset_mean - true_offset).abs() < 0.05);
        // Varianz sollte abnehmen
        assert!(offset.offset_variance < 0.1);
    }

    #[test]
    fn kalman_update_with_noisy_measurements() {
        let mut offset = TimeOffset::new();
        let true_offset = 0.3;
        
        // Messungen mit simuliertem Rauschen
        let measurements = vec![
            (1.0, 1.32, 0.02),  // (t_local, t_global_measured, variance)
            (2.0, 2.28, 0.02),
            (3.0, 3.31, 0.02),
            (4.0, 4.29, 0.02),
        ];
        
        for (t_local, measurement, var) in measurements {
            offset.kalman_update(measurement, var, t_local);
        }
        
        // Trotz Rauschen sollte Schätzung plausibel sein
        assert!((offset.offset_mean - true_offset).abs() < 0.1);
    }

    #[test]
    fn predict_and_update_cycle() {
        let mut offset = TimeOffset::with_values(0.1, 0.05, 1.0);
        
        // Predict-Update-Zyklus
        offset.predict(1.0, 0.001);
        let var_after_predict = offset.offset_variance;
        
        offset.kalman_update(10.2, 0.01, 10.0);
        let var_after_update = offset.offset_variance;
        
        // Predict erhöht Varianz, Update reduziert sie
        assert!(var_after_predict > 0.05);
        assert!(var_after_update < var_after_predict);
    }

    #[test]
    fn variance_bounds_enforced() {
        let mut offset = TimeOffset::new();
        
        // Test untere Grenze
        offset.kalman_update(10.0, 1e-12, 10.0);
        offset.kalman_update(20.0, 1e-12, 20.0);
        assert!(offset.offset_variance >= 1e-6);
        
        // Test obere Grenze
        let mut offset2 = TimeOffset::new();
        for _ in 0..1000 {
            offset2.predict(1.0, 1.0);
        }
        assert!(offset2.offset_variance <= 10.0);
    }
}
