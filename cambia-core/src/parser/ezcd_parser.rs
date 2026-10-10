use regex::Regex;

use crate::{extract::{Extractor, Quartet, ReleaseInfo, Ripper, TrackExtractor}, integrity::IntegrityChecker, toc::{Toc, TocEntry, TocRaw}, track::{TestAndCopy, TrackEntry}, translate::{Translator, TranslatorCombined}, util::Time};
use simple_text_decode::DecodedText;

use super::{Parser, ParsedLog, ParserCombined, ParsedLogCombined, ParserTrack};

lazy_static! {
    static ref RIPPER_VERSION: Regex = Regex::new(r"EZ CD Audio Converter (\S+)").unwrap();
    static ref RELEASE_INFO: Regex = Regex::new(r"(?m)^(?P<artist>.+?) - (?P<title>.+?)\s*$").unwrap();
    static ref USED_DRIVE: Regex = Regex::new(r"(?m)^Device:\s*(?:\[[^\]]*\])?\s*(?P<drive>.+?)\s*$").unwrap();
    // Neither the mount point nor the trailing firmware is part of the AccurateRip name
    static ref DRIVE_TRIM: Regex = Regex::new(r"\s+[0-9A-Za-z.]{4}$").unwrap();
    static ref SETTING: Regex = Regex::new(r"(?m)^(?P<key>[^:\n]+?)\s*:+\s*(?P<value>.+?)\s*$").unwrap();

    static ref TOC: Regex = Regex::new(r"(?m)^\s*\[.\]\s+(?P<track>\d+)\s+(?P<start>[\d:.]+)\s+(?P<length>[\d:.]+)\s+(?P<start_sector>\d+)\s+(?P<end_sector>\d+)(?P<rest>.*)$").unwrap();
    static ref PREGAP: Regex = Regex::new(r"(?P<time>\d+:\d+\.\d+)").unwrap();
    static ref PREEMPHASIS: Regex = Regex::new(r"(?P<boolean>Yes|No)\s*$").unwrap();

    static ref TRACKS: Regex = Regex::new(r#"(?m)^\s*(?P<track>\d+)\s+(?P<status>\S+)\s+(?P<copy>[0-9A-Fa-f]{8})(?:\s+(?P<test>[0-9A-Fa-f]{8}))?\s+(?P<errors>\d+(?:\s+\d+)*)\s+"(?P<filename>.*)""#).unwrap();
}

pub struct EzcdParser {
    encoded_log: DecodedText,
}

struct EzcdParserSingle {
    log: String,
    language: String,
}

struct EzcdParserTrack {
    num: u8,
    status: String,
    copy_crc: String,
    test_crc: String,
    filename: String,
    pregap: Option<Time>,
    preemphasis: Option<bool>,
}

impl EzcdParser {
    pub fn new(encoded_log: DecodedText) -> EzcdParser {
        EzcdParser {
            encoded_log,
        }
    }
}

impl EzcdParserSingle {
    pub fn new(log: String) -> EzcdParserSingle {
        let (language, _) = EzcdParserSingle::translate(log.clone());
        EzcdParserSingle {
            log,
            language,
        }
    }

    fn setting(&self, key: &str) -> Option<String> {
        SETTING.captures_iter(&self.log)
            .find(|captures| captures["key"].trim() == key)
            .map(|captures| captures["value"].trim().to_owned())
    }

    fn boolean_matcher(&self, key: &str) -> Quartet {
        match self.setting(key).as_deref() {
            Some("Enabled") => Quartet::True,
            Some("Disabled") => Quartet::False,
            _ => Quartet::Unknown,
        }
    }

    fn toc_extras(&self) -> Vec<(u8, Option<Time>, Option<bool>)> {
        TOC.captures_iter(&self.log).filter_map(|captures| {
            let num = captures["track"].parse::<u8>().ok()?;
            let rest = &captures["rest"];
            let pregap = PREGAP.captures(rest).map(|pregap| Time::from_mm_ss_ff(&pregap["time"]));
            let preemphasis = PREEMPHASIS.captures(rest).map(|preemphasis| &preemphasis["boolean"] == "Yes");
            Some((num, pregap, preemphasis))
        }).collect()
    }
}

impl ParserCombined for EzcdParser {
    fn parse_combined(&self) -> ParsedLogCombined {
        let parsed_logs: Vec<ParsedLog> = vec![EzcdParserSingle::new(self.encoded_log.text.trim().to_string()).parse()];

        ParsedLogCombined {
            parsed_logs,
            encoding: self.encoded_log.orig_encoding.to_string()
        }
    }
}

impl TranslatorCombined for EzcdParser {
    fn translate_combined(&self) -> String {
        self.encoded_log.text.clone()
    }
}

impl Parser for EzcdParserSingle {}

impl Extractor for EzcdParserSingle {
    fn extract_ripper(&self) -> Ripper {
        Ripper::EZCD
    }

    fn extract_ripper_version(&self) -> String {
        let captures = RIPPER_VERSION.captures(&self.log);
        match captures {
            Some(captures) => captures.get(1).unwrap().as_str().to_string(),
            None => String::from("Unknown"),
        }
    }

    fn extract_language(&self) -> String {
        self.language.clone()
    }

    fn extract_release_info(&self) -> ReleaseInfo {
        let relinfo = self.log
            .lines()
            .skip_while(|line| !line.starts_with("Log creation date"))
            .skip(1)
            .find(|line| !line.trim().is_empty());

        match relinfo.and_then(|line| RELEASE_INFO.captures(line)) {
            Some(captures) => ReleaseInfo::new(captures["artist"].trim().to_owned(), captures["title"].trim().to_owned()),
            None => ReleaseInfo::default(),
        }
    }

    fn extract_drive(&self) -> String {
        let captures = USED_DRIVE.captures(&self.log);
        match captures {
            Some(captures) => DRIVE_TRIM.replace(captures["drive"].trim(), "").trim().to_string(),
            None => String::default(),
        }
    }

    fn extract_read_offset(&self) -> Option<i16> {
        self.setting("Sample offset").and_then(|offset| offset.parse::<i16>().ok())
    }

    fn extract_use_c2(&self) -> Quartet {
        self.boolean_matcher("C2 pointers")
    }

    fn extract_test_and_copy(&self) -> Quartet {
        self.boolean_matcher("Verify audio")
    }

    // 10.3.0 has no Apply line, but gain that was never calculated was never applied
    fn extract_normalize(&self) -> Quartet {
        match (self.boolean_matcher("Apply ReplayGain"), self.boolean_matcher("Calculate ReplayGain")) {
            (Quartet::Unknown, Quartet::False) => Quartet::False,
            (apply, _) => apply,
        }
    }

    fn extract_audio_encoder(&self) -> Vec<String> {
        match self.setting("Encoder") {
            Some(encoder) => vec![encoder],
            None => Vec::new(),
        }
    }

    fn extract_toc(&self) -> Toc {
        let entries: Vec<TocEntry> = TOC.captures_iter(&self.log).filter_map(|captures| {
            Some(TocEntry::new(
                captures["track"].parse::<u32>().ok()?,
                Time::from_mm_ss_ff(&captures["start"]),
                Time::from_mm_ss_ff(&captures["length"]),
                captures["start_sector"].parse::<u32>().ok()?,
                captures["end_sector"].parse::<u32>().ok()?,
            ))
        }).collect();

        Toc::new(TocRaw::new(entries))
    }

    fn extract_tracks(&self) -> Vec<TrackEntry> {
        let toc_extras = self.toc_extras();

        TRACKS.captures_iter(&self.log).filter_map(|captures| {
            let num = captures["track"].parse::<u8>().ok()?;
            let extras = toc_extras.iter().find(|(track, _, _)| *track == num);
            let track_parser = EzcdParserTrack {
                num,
                status: captures["status"].to_owned(),
                copy_crc: captures["copy"].to_uppercase(),
                test_crc: captures.name("test").map(|test| test.as_str().to_uppercase()).unwrap_or_default(),
                filename: captures["filename"].to_owned(),
                pregap: extras.and_then(|(_, pregap, _)| *pregap),
                preemphasis: extras.and_then(|(_, _, preemphasis)| *preemphasis),
            };
            Some(track_parser.parse_track())
        }).collect()
    }
}

impl Translator for EzcdParserSingle {
    fn translate(log: String) -> (String, String) {
        (String::from("English"), log)
    }
}

impl IntegrityChecker for EzcdParserSingle {}

impl ParserTrack for EzcdParserTrack {}

impl TrackExtractor for EzcdParserTrack {
    fn extract_num(&self) -> u8 {
        self.num
    }

    fn extract_is_range(&self) -> bool {
        false
    }

    fn extract_is_aborted(&self) -> bool {
        self.status != "Success"
    }

    fn extract_filenames(&self) -> Vec<String> {
        if self.filename.is_empty() { Vec::new() } else { vec![self.filename.clone()] }
    }

    fn extract_pregap_length(&self) -> Option<Time> {
        self.pregap
    }

    fn extract_preemphasis(&self) -> Option<bool> {
        self.preemphasis
    }

    fn extract_test_and_copy(&self) -> TestAndCopy {
        TestAndCopy::new_no_skipzero(self.test_crc.clone(), self.copy_crc.clone())
    }
}
