#![deny(missing_docs)]

use crate::error::Error;
use crate::iokit::CoreDisplay_DisplayCreateInfoDictionary;
use crate::iokit::IoObject;
use crate::{arm, intel};
use core_foundation::base::{CFType, TCFType};
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::display::CGDisplay;
use ddc::{
    DdcCommand, DdcCommandMarker, DdcCommandRaw, DdcCommandRawMarker, DdcHost, Delay, ErrorCode, I2C_ADDRESS_DDC_CI,
    SUB_ADDRESS_DDC_CI,
};
use std::time::Duration;
use std::{fmt, iter};

/// DDC access method for a monitor
#[derive(Debug)]
enum MonitorService {
    Intel(IoObject),
    Arm(arm::IOAVService),
}

/// A handle to an attached monitor that allows the use of DDC/CI operations.
#[derive(Debug)]
pub struct Monitor {
    monitor: CGDisplay,
    service: MonitorService,
    i2c_address: u16,
    delay: Delay,
}

impl fmt::Display for Monitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Monitor {
    /// Create a new monitor from the specified handle.
    fn new(monitor: CGDisplay, service: MonitorService, i2c_address: u16) -> Self {
        Monitor {
            monitor,
            service,
            i2c_address,
            delay: Default::default(),
        }
    }

    /// Enumerate all connected physical monitors returning [Vec<Monitor>]
    pub fn enumerate() -> Result<Vec<Self>, Error> {
        let monitors = CGDisplay::active_displays()
            .map_err(Error::from)?
            .into_iter()
            .filter_map(|display_id| {
                let display = CGDisplay::new(display_id);
                #[cfg(target_arch = "aarch64")]
                {
                    if let Ok((service, i2c_address)) = arm::get_display_av_service(display) {
                        return Some(Self::new(display, MonitorService::Arm(service), i2c_address));
                    }
                    if let Some(service) = intel::get_io_framebuffer_port(display) {
                        return Some(Self::new(display, MonitorService::Intel(service), I2C_ADDRESS_DDC_CI));
                    }
                    None
                }
                #[cfg(not(target_arch = "aarch64"))]
                {
                    if let Some(service) = intel::get_io_framebuffer_port(display) {
                        return Some(Self::new(display, MonitorService::Intel(service), I2C_ADDRESS_DDC_CI));
                    }
                    if let Ok((service, i2c_address)) = arm::get_display_av_service(display) {
                        return Some(Self::new(display, MonitorService::Arm(service), i2c_address));
                    }
                    None
                }
            })
            .collect();
        Ok(monitors)
    }

    /// Physical monitor description string. If it cannot get the product's name it will use
    /// the vendor number and model number to form a description
    pub fn description(&self) -> String {
        self.product_name().unwrap_or(format!(
            "{:04x}:{:04x}",
            self.monitor.vendor_number(),
            self.monitor.model_number()
        ))
    }

    /// Serial number for this [Monitor]
    pub fn serial_number(&self) -> Option<String> {
        let serial = self.monitor.serial_number();
        match serial {
            0 => None,
            _ => Some(format!("{}", serial)),
        }
    }

    /// Product name for this [Monitor], if available
    pub fn product_name(&self) -> Option<String> {
        let info: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(CoreDisplay_DisplayCreateInfoDictionary(self.monitor.id)) };

        let display_product_name_key = CFString::from_static_string("DisplayProductName");
        let display_product_names_dict = info.find(&display_product_name_key)?.downcast::<CFDictionary>()?;
        let (_, localized_product_names) = display_product_names_dict.get_keys_and_values();
        localized_product_names
            .first()
            .map(|name| unsafe { CFString::wrap_under_get_rule(*name as CFStringRef) }.to_string())
    }

    /// Returns Extended display identification data (EDID) for this [Monitor] as raw bytes data
    pub fn edid(&self) -> Option<Vec<u8>> {
        let info: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(CoreDisplay_DisplayCreateInfoDictionary(self.monitor.id)) };
        let display_product_name_key = CFString::from_static_string("IODisplayEDIDOriginal");
        let edid_data = info.find(&display_product_name_key)?.downcast::<CFData>()?;
        Some(edid_data.bytes().into())
    }

    /// CoreGraphics display handle for this monitor
    pub fn handle(&self) -> CGDisplay {
        self.monitor
    }

    fn encode_command<'a>(&self, data: &[u8], packet: &'a mut [u8]) -> &'a [u8] {
        packet[0] = SUB_ADDRESS_DDC_CI;
        packet[1] = 0x80 | data.len() as u8;
        packet[2..2 + data.len()].copy_from_slice(data);
        packet[2 + data.len()] =
            Self::checksum(iter::once((self.i2c_address as u8) << 1).chain(packet[..2 + data.len()].iter().cloned()));
        &packet[..3 + data.len()]
    }

    fn decode_response<'a>(&self, response: &'a mut [u8]) -> Result<&'a mut [u8], crate::error::Error> {
        if response.is_empty() {
            return Ok(response);
        };

        if let Some((start, end)) = self.decode_response_bounds(response, 1, true) {
            return Ok(&mut response[start..end]);
        }
        if let Some((start, end)) = self.decode_response_bounds(response, 0, true) {
            return Ok(&mut response[start..end]);
        }
        if let Some((start, end)) = self.decode_response_bounds(response, 1, false) {
            return Ok(&mut response[start..end]);
        }
        if let Some((start, end)) = self.decode_response_bounds(response, 0, false) {
            return Ok(&mut response[start..end]);
        }

        let preview_len = response.len().min(16);
        Err(Error::Ddc(ErrorCode::Invalid(format!(
            "Invalid DDC/CI frame: {:02X?}",
            &response[..preview_len]
        ))))
    }

    fn decode_response_bounds(
        &self,
        response: &[u8],
        len_index: usize,
        require_length_flag: bool,
    ) -> Option<(usize, usize)> {
        let len_byte = *response.get(len_index)?;
        if require_length_flag && (len_byte & 0x80) == 0 {
            return None;
        }
        let len = (len_byte & 0x7f) as usize;
        let payload_start = len_index + 1;
        let checksum_index = payload_start + len;
        if checksum_index >= response.len() {
            return None;
        }

        let checksum = Self::checksum(
            iter::once(((self.i2c_address << 1) | 1) as u8)
                .chain(iter::once(SUB_ADDRESS_DDC_CI))
                .chain(response[len_index..payload_start + len].iter().cloned()),
        );
        if response[checksum_index] != checksum {
            return None;
        }

        Some((payload_start, payload_start + len))
    }

    fn extract_get_vcp_payload_from_raw(&self, request: &[u8], response: &[u8]) -> Option<[u8; 8]> {
        // Get VCP Feature request payload is [0x01, feature_code].
        if request.len() < 2 || request[0] != 0x01 {
            return None;
        }
        let expected_code = request[1];
        if response.len() < 8 {
            return None;
        }

        let reply_address = ((self.i2c_address << 1) | 1) as u8;

        // Strict Lunar-like validation against 11-byte raw frame layout:
        // [source, len, 0x02, result, code, type, max_hi, max_lo, cur_hi, cur_lo, checksum]
        for start in 0..=response.len() - 11 {
            let frame = &response[start..start + 11];
            if frame[2] != 0x02 || frame[4] != expected_code {
                continue;
            }
            let checksum = Self::checksum(
                iter::once(reply_address)
                    .chain(iter::once(SUB_ADDRESS_DDC_CI))
                    .chain(frame[1..10].iter().cloned()),
            );
            if frame[10] == checksum {
                return Some([
                    frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8], frame[9],
                ]);
            }
        }

        // Variant seen on some adapters: no explicit source byte in the returned frame:
        // [len, 0x02, result, code, type, max_hi, max_lo, cur_hi, cur_lo, checksum]
        if response.len() >= 10 {
            for start in 0..=response.len() - 10 {
                let frame = &response[start..start + 10];
                if frame[1] != 0x02 || frame[3] != expected_code {
                    continue;
                }
                let checksum = Self::checksum(
                    iter::once(reply_address)
                        .chain(iter::once(SUB_ADDRESS_DDC_CI))
                        .chain(frame[0..9].iter().cloned()),
                );
                if frame[9] == checksum {
                    return Some([
                        frame[1], frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8],
                    ]);
                }
            }
        }

        // Direct 8-byte payload already present in stream.
        for start in 0..=response.len() - 8 {
            let payload = &response[start..start + 8];
            if payload[0] == 0x02 && payload[2] == expected_code {
                return Some([
                    payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6], payload[7],
                ]);
            }
        }

        // Relaxed fallback: keep the same frame layout but ignore checksum.
        if response.len() >= 11 {
            for start in 0..=response.len() - 11 {
                let frame = &response[start..start + 11];
                if frame[2] == 0x02 && frame[4] == expected_code {
                    return Some([
                        frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8], frame[9],
                    ]);
                }
            }
        }

        if response.len() >= 10 {
            for start in 0..=response.len() - 10 {
                let frame = &response[start..start + 10];
                if frame[1] == 0x02 && frame[3] == expected_code {
                    return Some([
                        frame[1], frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8],
                    ]);
                }
            }
        }

        for start in 0..=response.len() - 8 {
            let payload = &response[start..start + 8];
            if payload[0] == 0x02 && (payload[1] == 0x00 || payload[1] == 0x01) {
                return Some([
                    payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6], payload[7],
                ]);
            }
        }

        None
    }
}

impl DdcHost for Monitor {
    type Error = Error;

    fn sleep(&mut self) {
        self.delay.sleep()
    }
}

impl DdcCommandRaw for Monitor {
    fn execute_raw<'a>(
        &mut self,
        data: &[u8],
        out: &'a mut [u8],
        response_delay: Duration,
    ) -> Result<&'a mut [u8], Self::Error> {
        assert!(data.len() <= 36);
        let response_delay = response_delay.max(Duration::from_millis(120));
        let mut packet = [0u8; 36 + 3];
        let packet = self.encode_command(data, &mut packet);
        if out.is_empty() {
            let response = match &self.service {
                MonitorService::Intel(service) => {
                    intel::execute(service, self.i2c_address, packet, out, response_delay)
                }
                MonitorService::Arm(service) => arm::execute(service, self.i2c_address, packet, out, response_delay),
            }?;
            return self.decode_response(response);
        }

        // Some adapters return extra status/padding bytes and need larger buffers.
        let mut raw_response_buffer = [0u8; 128];
        let is_get_vcp = data.len() >= 2 && data[0] == 0x01;
        let mut last_error: Option<crate::error::Error> = None;

        // Get VCP reads are retried once with a slower response delay because some
        // monitors return truncated replies on the first read window.
        for extra_delay_ms in [0_u64, 100_u64] {
            if extra_delay_ms > 0 && !is_get_vcp {
                continue;
            }
            let attempt_delay = response_delay + Duration::from_millis(extra_delay_ms);
            let response = match &self.service {
                MonitorService::Intel(service) => {
                    intel::execute(service, self.i2c_address, packet, &mut raw_response_buffer, attempt_delay)
                }
                MonitorService::Arm(service) => {
                    arm::execute(service, self.i2c_address, packet, &mut raw_response_buffer, attempt_delay)
                }
            }?;

            if let Some(payload) = self.extract_get_vcp_payload_from_raw(data, response) {
                let copy_len = payload.len().min(out.len());
                out[..copy_len].copy_from_slice(&payload[..copy_len]);
                return Ok(&mut out[..copy_len]);
            }

            let decoded = match self.decode_response(response) {
                Ok(decoded) => decoded,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            };
            let normalized = normalize_get_vcp_payload(data, decoded);

            if is_get_vcp {
                if normalized.len() < 8 {
                    continue;
                }
                out[..8].copy_from_slice(&normalized[..8]);
                return Ok(&mut out[..8]);
            }

            let copy_len = normalized.len().min(out.len());
            out[..copy_len].copy_from_slice(&normalized[..copy_len]);
            return Ok(&mut out[..copy_len]);
        }

        if let Some(error) = last_error {
            return Err(error);
        }

        // No parseable frame came back after retries.
        Err(Error::Ddc(ErrorCode::Invalid(String::from(
            "Unable to parse DDC/CI response payload",
        ))))
    }
}

fn normalize_get_vcp_payload<'a>(request: &[u8], response: &'a mut [u8]) -> &'a mut [u8] {
    // Get VCP Feature request payload is [0x01, feature_code]
    if request.len() < 2 || request[0] != 0x01 {
        return response;
    }
    let expected_code = request[1];
    if response.len() == 8 && response[0] == 0x02 {
        return response;
    }
    if response.len() >= 8 {
        // Some bridges insert one or more bytes between result and feature code:
        // [02, result, noise..., feature, type, max_hi, max_lo, value_hi, value_lo]
        // Normalize in-place when the expected feature code is still near the start.
        if response[0] == 0x02 {
            let mut expected_pos = None;
            let scan_end = response.len().min(5);
            for index in 2..scan_end {
                if response[index] == expected_code {
                    expected_pos = Some(index);
                    break;
                }
            }
            if let Some(index) = expected_pos {
                if index > 2 && response.len() >= index + 6 {
                    response.copy_within(index..index + 6, 2);
                    return &mut response[..8];
                }
            }
        }

        // Strict match: opcode + requested VCP code in MCCS-compliant position.
        for start in 0..=response.len() - 8 {
            let candidate = &response[start..start + 8];
            if candidate[0] == 0x02 && candidate[2] == expected_code {
                return &mut response[start..start + 8];
            }
        }
        // Relaxed match: some adapters prepend status noise but still carry a
        // valid 8-byte Get VCP reply frame.
        for start in 0..=response.len() - 8 {
            let candidate = &response[start..start + 8];
            if candidate[0] == 0x02 && (candidate[1] == 0x00 || candidate[1] == 0x01) {
                return &mut response[start..start + 8];
            }
        }
        // Final fallback for malformed adapters: return a fixed-size frame so
        // higher layers can parse semantic errors instead of failing on length.
        if response[0] == 0x02 {
            return &mut response[..8];
        }
        for start in 0..=response.len() - 8 {
            if response[start] == 0x02 {
                return &mut response[start..start + 8];
            }
        }
    }
    if response.len() >= 9 {
        // Some bridges inject one leading byte before the Get VCP opcode.
        for start in 0..=response.len() - 9 {
            let shifted = &response[start + 1..start + 9];
            if shifted[0] == 0x02 && (shifted[1] == 0x00 || shifted[1] == 0x01) {
                return &mut response[start + 1..start + 9];
            }
        }
    }
    response
}

impl DdcCommandMarker for Monitor {}

impl DdcCommandRawMarker for Monitor {
    fn set_sleep_delay(&mut self, delay: Delay) {
        self.delay = delay;
    }
}
