//! Calibration certificates: what is printed on one, the organization's template, and
//! how certificate numbers are formed.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{ConditionReceived, ConditionReturned, text_enum};
use crate::{Error, FieldError, Id, Result};

text_enum!(
    /// Internal: one of the organization's own items. External: a customer's equipment.
    CertificateKind { Internal => "internal", External => "external" }
);

text_enum!(
    /// Where the logo sits in the certificate's header.
    CertificateHeaderLayout { LogoLeft => "logo_left", LogoCenter => "logo_center" }
);

text_enum!(
    /// The frame drawn around the page.
    CertificateBorder { None => "none", Single => "single", Double => "double", Thick => "thick" }
);

text_enum!(
    CertificateFont { Sans => "sans", Serif => "serif" }
);

text_enum!(
    CertificatePaper { Letter => "letter", A4 => "a4" }
);

text_enum!(
    TemperatureUnit { Celsius => "celsius", Fahrenheit => "fahrenheit" }
);

text_enum!(
    CertificateLogoSize { Small => "small", Medium => "medium", Large => "large" }
);

impl CertificatePaper {
    /// Width and height in inches.
    pub fn inches(self) -> (f64, f64) {
        match self {
            CertificatePaper::Letter => (8.5, 11.0),
            CertificatePaper::A4 => (8.268, 11.693),
        }
    }
}

/// One piece of equipment (a standard) the item was calibrated against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CertificateEquipment {
    /// Set when picked from Instruments & Systems.
    #[serde(default)]
    pub asset_id: Option<Id>,
    #[serde(default)]
    pub asset_tag: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub model_number: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub cal_due: Option<NaiveDate>,
}

/// Everything entered on the Generate Certificate page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CertificateDetails {
    pub kind: CertificateKind,
    /// Internal certificates: the item, when picked from Instruments & Systems.
    #[serde(default)]
    pub asset_id: Option<Id>,
    #[serde(default)]
    pub customer_name: String,
    #[serde(default)]
    pub customer_address: String,
    #[serde(default)]
    pub asset_tag: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub model_number: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub procedure: String,
    /// As entered (e.g. "21.5" or "20 ± 2"); the unit comes from the template.
    #[serde(default)]
    pub temperature: String,
    /// Relative humidity in percent, as entered.
    #[serde(default)]
    pub humidity: String,
    pub cal_date: NaiveDate,
    #[serde(default)]
    pub next_cal_date: Option<NaiveDate>,
    #[serde(default)]
    pub condition_received: Option<ConditionReceived>,
    #[serde(default)]
    pub condition_returned: Option<ConditionReturned>,
    #[serde(default)]
    pub equipment: Vec<CertificateEquipment>,
    #[serde(default)]
    pub calibrated_by: String,
    /// The PolarisCal user who calibrated it, when picked (their signature is printed).
    #[serde(default)]
    pub calibrated_by_id: Option<Id>,
    /// Left blank, the person who calibrated it approves it too.
    #[serde(default)]
    pub approved_by: String,
    #[serde(default)]
    pub approved_by_id: Option<Id>,
    /// The signers' signature images (data URLs) as they were when the certificate was
    /// issued. Filled in by the store from the users' saved signatures, never from input.
    #[serde(default)]
    pub calibrated_by_signature: Option<String>,
    #[serde(default)]
    pub approved_by_signature: Option<String>,
    /// The name of the test data file appended to the PDF when it was issued, if any.
    #[serde(default)]
    pub test_data_file: String,
}

fn trim(s: &mut String) {
    let t = s.trim();
    if t.len() != s.len() {
        *s = t.to_string();
    }
}

impl CertificateDetails {
    /// Trims the text and checks what a certificate can't be issued without.
    pub fn normalize(&mut self) -> Result<()> {
        for s in [
            &mut self.customer_name,
            &mut self.customer_address,
            &mut self.asset_tag,
            &mut self.manufacturer,
            &mut self.description,
            &mut self.model_number,
            &mut self.serial_number,
            &mut self.procedure,
            &mut self.temperature,
            &mut self.humidity,
            &mut self.calibrated_by,
            &mut self.approved_by,
            &mut self.test_data_file,
        ] {
            trim(s);
        }
        if self.calibrated_by.is_empty() {
            self.calibrated_by_id = None;
        }
        if self.approved_by.is_empty() {
            self.approved_by = self.calibrated_by.clone();
            self.approved_by_id = self.calibrated_by_id;
        }
        for e in &mut self.equipment {
            for s in [
                &mut e.asset_tag,
                &mut e.description,
                &mut e.model_number,
                &mut e.serial_number,
            ] {
                trim(s);
            }
        }
        self.equipment
            .retain(|e| !(e.asset_tag.is_empty() && e.description.is_empty()));
        match self.kind {
            CertificateKind::Internal => {
                self.customer_name.clear();
                self.customer_address.clear();
            }
            CertificateKind::External => {
                self.asset_id = None;
                self.asset_tag.clear();
            }
        }

        let mut errors = Vec::new();
        match self.kind {
            CertificateKind::External if self.customer_name.is_empty() => {
                errors.push(FieldError::new(
                    "customer_name",
                    "Customer name is required.",
                ));
            }
            CertificateKind::Internal if self.asset_tag.is_empty() => {
                errors.push(FieldError::new("asset_tag", "Asset number is required."));
            }
            _ => {}
        }
        if self.description.is_empty() {
            errors.push(FieldError::new(
                "description",
                "Product description is required.",
            ));
        }
        if self.next_cal_date.is_some_and(|next| next < self.cal_date) {
            errors.push(FieldError::new(
                "next_cal_date",
                "The next calibration can't be before the calibration date.",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(errors))
        }
    }

    /// Who or what the certificate is for, for lists: the asset number or the customer.
    pub fn subject(&self) -> String {
        match self.kind {
            CertificateKind::Internal => self.asset_tag.clone(),
            CertificateKind::External => self.customer_name.clone(),
        }
    }
}

/// The organization's certificate layout and numbering (Settings > Certificate Template).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CertificateTemplate {
    pub title: String,
    pub organization_name: String,
    pub organization_address: String,
    /// Phone, email, web site: one line under the address.
    pub organization_contact: String,
    /// The logo as a `data:image/...;base64,` URL, or None.
    pub logo: Option<String>,
    pub logo_size: CertificateLogoSize,
    pub header_layout: CertificateHeaderLayout,
    /// Titles, rules and table headings, as `#rrggbb`.
    pub accent_color: String,
    pub border: CertificateBorder,
    pub font: CertificateFont,
    pub paper: CertificatePaper,
    pub temperature_unit: TemperatureUnit,
    /// The traceability statement printed above the sign-off lines.
    pub statement: String,
    /// Small print at the bottom of the page.
    pub footer: String,
    /// The certificate number, with `{SEQ}` for the counter and optionally `{YYYY}`, `{YY}`,
    /// `{MM}` and `{DD}` for the date it's issued, e.g. `PC-{YYYY}-{SEQ}`.
    pub number_pattern: String,
    /// The counter is padded with zeros to this many digits.
    pub number_digits: u8,
    /// Start the counter again at 1 each January.
    pub number_reset_yearly: bool,
    /// The counter for the next certificate (stored apart from the template, so saving the
    /// template while someone issues a certificate doesn't reuse a number).
    pub next_number: u32,
}

impl Default for CertificateTemplate {
    fn default() -> Self {
        Self {
            title: "Certificate of Calibration".into(),
            organization_name: String::new(),
            organization_address: String::new(),
            organization_contact: String::new(),
            logo: None,
            logo_size: CertificateLogoSize::Medium,
            header_layout: CertificateHeaderLayout::LogoLeft,
            accent_color: "#1f4e79".into(),
            border: CertificateBorder::Single,
            font: CertificateFont::Sans,
            paper: CertificatePaper::Letter,
            temperature_unit: TemperatureUnit::Celsius,
            statement:
                "The instrument identified above was calibrated using the equipment listed, \
                        whose calibration is traceable to national or international measurement \
                        standards. The results relate only to the item calibrated."
                    .into(),
            footer: "This certificate shall not be reproduced except in full without the written \
                     approval of the issuing organization."
                .into(),
            number_pattern: "CAL-{YYYY}-{SEQ}".into(),
            number_digits: 4,
            number_reset_yearly: true,
            next_number: 1,
        }
    }
}

/// The largest logo or signature kept (as the encoded data URL).
pub const MAX_IMAGE_BYTES: usize = 1_400_000;

/// Checks an image kept as a data URL (a logo or a signature).
pub fn check_image(field: &str, image: &str) -> Result<()> {
    if !image.starts_with("data:image/") {
        Err(Error::validation(field, "Choose an image file."))
    } else if image.len() > MAX_IMAGE_BYTES {
        Err(Error::validation(
            field,
            "That image is too large. Use one under 1 MB.",
        ))
    } else {
        Ok(())
    }
}

/// A PolarisCal user who can sign certificates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CertificateSigner {
    pub id: Id,
    pub display_name: String,
    pub has_signature: bool,
}

impl CertificateTemplate {
    pub fn normalize(&mut self) -> Result<()> {
        for s in [
            &mut self.title,
            &mut self.organization_name,
            &mut self.organization_address,
            &mut self.organization_contact,
            &mut self.statement,
            &mut self.footer,
            &mut self.number_pattern,
            &mut self.accent_color,
        ] {
            trim(s);
        }
        if self.logo.as_deref().is_some_and(|l| l.trim().is_empty()) {
            self.logo = None;
        }
        let mut errors = Vec::new();
        if self.title.is_empty() {
            errors.push(FieldError::new(
                "title",
                "Enter a title for the certificate.",
            ));
        }
        if !is_hex_color(&self.accent_color) {
            errors.push(FieldError::new("accent_color", "Use a color like #1f4e79."));
        }
        if let Some(Err(Error::Validation(mut e))) =
            self.logo.as_deref().map(|l| check_image("logo", l))
        {
            errors.append(&mut e);
        }
        if !self.number_pattern.contains("{SEQ}") {
            errors.push(FieldError::new(
                "number_pattern",
                "The number format must include {SEQ} for the counter.",
            ));
        } else if self.number_pattern.chars().count() > 40 {
            errors.push(FieldError::new(
                "number_pattern",
                "Keep the number format to 40 characters.",
            ));
        }
        if !(1..=8).contains(&self.number_digits) {
            errors.push(FieldError::new(
                "number_digits",
                "Use between 1 and 8 digits.",
            ));
        }
        if self.next_number == 0 {
            errors.push(FieldError::new(
                "next_number",
                "The next number must be at least 1.",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(errors))
        }
    }

    /// The number a certificate issued on `date` with counter `seq` gets.
    pub fn format_number(&self, seq: u32, date: NaiveDate) -> String {
        format_certificate_number(&self.number_pattern, self.number_digits, seq, date)
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Fills in a certificate number pattern (see [`CertificateTemplate::number_pattern`]).
pub fn format_certificate_number(pattern: &str, digits: u8, seq: u32, date: NaiveDate) -> String {
    pattern
        .replace("{YYYY}", &format!("{:04}", date.year()))
        .replace("{YY}", &format!("{:02}", date.year().rem_euclid(100)))
        .replace("{MM}", &format!("{:02}", date.month()))
        .replace("{DD}", &format!("{:02}", date.day()))
        .replace(
            "{SEQ}",
            &format!("{:0width$}", seq, width = digits as usize),
        )
}

/// An issued certificate as listed: enough to find it again.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CertificateSummary {
    pub id: Id,
    pub certificate_number: String,
    pub kind: CertificateKind,
    pub asset_id: Option<Id>,
    /// The asset number (internal) or customer (external).
    pub subject: String,
    pub description: String,
    pub cal_date: NaiveDate,
    pub issued_by: String,
    pub created_at: DateTime<Utc>,
}

/// An issued certificate with what was printed on it, including the template as it was,
/// so a reprint looks the same even after the template changes.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Certificate {
    #[serde(flatten)]
    pub summary: CertificateSummary,
    pub details: CertificateDetails,
    pub template: CertificateTemplate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CertificateFilter {
    /// Matches the number, asset number, customer, or description.
    pub search: Option<String>,
    pub asset_id: Option<Id>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn numbers_fill_in_the_pattern() {
        assert_eq!(
            format_certificate_number("CAL-{YYYY}-{SEQ}", 4, 7, day(2026, 3, 9)),
            "CAL-2026-0007"
        );
        assert_eq!(
            format_certificate_number("{YY}{MM}{DD}/{SEQ}", 3, 1234, day(2026, 3, 9)),
            "260309/1234"
        );
    }

    #[test]
    fn templates_need_a_counter_and_a_color() {
        let mut t = CertificateTemplate::default();
        assert!(t.normalize().is_ok());
        t.number_pattern = "CAL-{YYYY}".into();
        t.accent_color = "blue".into();
        let Err(Error::Validation(errors)) = t.normalize() else {
            panic!("expected validation errors")
        };
        let fields: Vec<_> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["accent_color", "number_pattern"]);
    }

    fn details(kind: CertificateKind) -> CertificateDetails {
        CertificateDetails {
            kind,
            asset_id: Some(4),
            customer_name: " Acme ".into(),
            customer_address: String::new(),
            asset_tag: "DMM-1".into(),
            manufacturer: String::new(),
            description: "Multimeter".into(),
            model_number: String::new(),
            serial_number: String::new(),
            procedure: String::new(),
            temperature: String::new(),
            humidity: String::new(),
            cal_date: day(2026, 3, 9),
            next_cal_date: Some(day(2027, 3, 9)),
            condition_received: None,
            condition_returned: None,
            equipment: vec![CertificateEquipment::default()],
            calibrated_by: "Tech".into(),
            calibrated_by_id: Some(2),
            approved_by: String::new(),
            approved_by_id: None,
            calibrated_by_signature: None,
            approved_by_signature: None,
            test_data_file: String::new(),
        }
    }

    #[test]
    fn details_keep_only_what_their_kind_uses() {
        let mut d = details(CertificateKind::External);
        d.normalize().unwrap();
        assert_eq!(d.customer_name, "Acme");
        assert_eq!((d.asset_id, d.asset_tag.as_str()), (None, ""));
        assert!(d.equipment.is_empty(), "blank equipment rows are dropped");
        assert_eq!(d.subject(), "Acme");
        assert_eq!(
            (d.approved_by.as_str(), d.approved_by_id),
            ("Tech", Some(2)),
            "a blank approver is the person who calibrated it"
        );

        let mut d = details(CertificateKind::Internal);
        d.normalize().unwrap();
        assert_eq!(d.customer_name, "");
        assert_eq!(d.subject(), "DMM-1");

        let mut d = details(CertificateKind::Internal);
        d.asset_tag = " ".into();
        d.next_cal_date = Some(day(2026, 1, 1));
        let Err(Error::Validation(errors)) = d.normalize() else {
            panic!("expected validation errors")
        };
        let fields: Vec<_> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["asset_tag", "next_cal_date"]);
    }
}
