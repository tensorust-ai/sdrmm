use super::{Heard, Tracker};
use crate::remote_id::{
    RemoteIdMessage, RemoteIdPhy, RemoteIdTransport, odid::encode::pack, synth::demo_drone,
};

fn pack_at(north_deg: f64, seconds: f32) -> Vec<u8> {
    let mut drone = demo_drone();
    for message in &mut drone {
        if let RemoteIdMessage::Location(location) = message {
            location.lat = location.lat.map(|lat| lat + north_deg);
            location.seconds_after_hour = location.seconds_after_hour.map(|at| at + seconds);
        }
    }
    pack(&drone)
}

/// One pack a second from one BLE address, at the given offsets north; the last frame's trust.
fn trust_after(norths: &[f64]) -> (String, Vec<String>) {
    let mut tracker = Tracker::default();
    let mut last = None;
    for (second, north) in norths.iter().enumerate() {
        let payload = pack_at(*north, second as f32);
        let frame = tracker
            .frame(Heard {
                transport: RemoteIdTransport::BluetoothExtended,
                phy: RemoteIdPhy::LeCodedS8,
                address: "D4:5A:21:0C:7E:19".to_owned(),
                channel: Some(5),
                counter: Some(second as u8),
                ssid: None,
                level_dbfs: -42.0,
                payload: &payload,
            })
            .expect("pack decodes");
        last = frame.trust;
        tracker.advance(1_000_000, 1e6);
    }
    let trust = last.expect("ODID frames are judged");
    (trust.verdict, trust.reasons)
}

#[test]
fn a_drone_flying_six_metres_a_second_stays_plausible() {
    let norths: Vec<f64> = (0..6).map(|step| f64::from(step) * 0.000_05).collect();
    assert_eq!(trust_after(&norths).0, "plausible");
}

#[test]
fn a_second_transmitter_on_the_same_address_111_km_away_is_spoofed() {
    let (verdict, reasons) = trust_after(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
    assert_eq!(verdict, "spoofed");
    assert!(
        reasons.iter().any(|reason| reason == "teleport"),
        "{reasons:?}"
    );
}
