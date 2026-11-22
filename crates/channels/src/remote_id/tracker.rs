use std::collections::VecDeque;

use crate::remote_id::RemoteIdFrame;

use crate::remote_id::RemoteIdMessage;

use crate::remote_id::RemoteIdPhy;

use crate::remote_id::RemoteIdTransport;

use crate::remote_id::RemoteIdTrust;

use remoteid::{Observation, Transmitter, Transport};

use super::odid;

const REMEMBERED: usize = 256;

pub(crate) struct Heard<'a> {
    pub transport: RemoteIdTransport,
    pub phy: RemoteIdPhy,
    pub address: String,
    pub channel: Option<u8>,
    pub counter: Option<u8>,
    pub ssid: Option<String>,
    pub level_dbfs: f32,
    pub payload: &'a [u8],
}

#[derive(Default)]
pub(crate) struct Tracker {
    ids: VecDeque<(String, String)>,
    rejected: u32,
    trust: remoteid::Tracker,
    samples: u64,
    now_ms: u64,
}

impl Tracker {
    pub(crate) fn rejected(&self) -> u32 {
        self.rejected
    }

    pub(crate) fn clear_rejected(&mut self) {
        self.rejected = 0;
    }

    pub(crate) fn refuse(&mut self) {
        self.rejected = self.rejected.saturating_add(1);
    }

    /// Moves the receiver clock on by `samples` at `rate_hz`; the trust checks run on it.
    pub(crate) fn advance(&mut self, samples: usize, rate_hz: f64) {
        self.samples = self.samples.saturating_add(samples as u64);
        self.now_ms = (self.samples as f64 * 1_000.0 / rate_hz) as u64;
    }

    pub(crate) fn frame(&mut self, heard: Heard<'_>) -> Option<RemoteIdFrame> {
        let Ok(messages) = odid::parse(heard.payload) else {
            self.rejected = self.rejected.saturating_add(1);
            return None;
        };
        let uas_id = match odid::uas_id(&messages) {
            Some(id) => {
                self.remember(&heard.address, &id);
                Some(id)
            }
            None => self.recall(&heard.address),
        };
        let trust = self.judge(&heard);
        Some(RemoteIdFrame {
            transport: heard.transport,
            phy: heard.phy,
            address: heard.address,
            channel: heard.channel,
            counter: heard.counter,
            uas_id,
            ssid: heard.ssid,
            level_dbfs: heard.level_dbfs,
            messages,
            trust,
        })
    }

    fn judge(&mut self, heard: &Heard<'_>) -> Option<RemoteIdTrust> {
        let transport = match heard.transport {
            RemoteIdTransport::BluetoothLegacy => Transport::Bluetooth4,
            RemoteIdTransport::BluetoothExtended => Transport::Bluetooth5,
            RemoteIdTransport::WifiBeacon => Transport::WifiBeacon,
            RemoteIdTransport::WifiNan => Transport::WifiNan,
            RemoteIdTransport::WifiBeaconFrench | RemoteIdTransport::WifiBeaconDji => return None,
        };
        let mac = mac(&heard.address)?;
        let from = match transport {
            Transport::WifiBeacon | Transport::WifiNan => Transmitter::Wifi(mac),
            _ => Transmitter::Ble(mac),
        };
        let observation = Observation::new(transport, heard.counter, heard.payload, self.now_ms);
        let assessment = self.trust.observe(from, &observation).assessment;
        Some(RemoteIdTrust {
            verdict: assessment.verdict.name().to_owned(),
            reasons: assessment
                .reasons
                .iter()
                .map(|reason| reason.name().to_owned())
                .collect(),
        })
    }

    pub(crate) fn direct(
        &mut self,
        heard: Heard<'_>,
        messages: Vec<RemoteIdMessage>,
    ) -> RemoteIdFrame {
        let uas_id = odid::uas_id(&messages);
        if let Some(id) = &uas_id {
            self.remember(&heard.address, id);
        }
        RemoteIdFrame {
            transport: heard.transport,
            phy: heard.phy,
            address: heard.address,
            channel: heard.channel,
            counter: heard.counter,
            uas_id,
            ssid: heard.ssid,
            level_dbfs: heard.level_dbfs,
            messages,
            trust: None,
        }
    }

    fn remember(&mut self, address: &str, id: &str) {
        if let Some(index) = self.ids.iter().position(|(known, _)| known == address) {
            self.ids.remove(index);
        }
        if self.ids.len() == REMEMBERED {
            self.ids.pop_front();
        }
        self.ids.push_back((address.to_owned(), id.to_owned()));
    }

    fn recall(&self, address: &str) -> Option<String> {
        self.ids
            .iter()
            .rev()
            .find(|(known, _)| known == address)
            .map(|(_, id)| id.clone())
    }
}

fn mac(address: &str) -> Option<[u8; 6]> {
    let mut bytes = [0; 6];
    let mut parts = address.split(':');
    for byte in &mut bytes {
        *byte = u8::from_str_radix(parts.next()?, 16).ok()?;
    }
    parts.next().is_none().then_some(bytes)
}

#[cfg(test)]
mod tests;
