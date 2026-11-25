use crate::types::TsNotifyEvent;
use std::collections::BTreeMap;

/// Escapes a raw string according to TeamSpeak ServerQuery specification.
pub fn escape_ts(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            ' ' => out.push_str("\\s"),
            '|' => out.push_str("\\p"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0B' => out.push_str("\\v"),
            c => out.push(c),
        }
    }
    out
}

/// Unescapes a TeamSpeak ServerQuery parameter value.
pub fn unescape_ts(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('s') => out.push(' '),
                Some('p') => out.push('|'),
                Some('a') => out.push('\x07'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0C'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('v') => out.push('\x0B'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Strips both `\n` and `\r` from ServerQuery lines (handles authentic `\n\r`, `\r\n`, and `\n`).
pub fn trim_ts_line(line: &str) -> &str {
    line.trim_matches(|c| c == '\r' || c == '\n')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsQueryStatus {
    pub id: u32,
    pub msg: String,
    pub extra_msg: Option<String>,
}

/// Parses an `error id=0 msg=ok` status line.
pub fn parse_error_line(raw_line: &str) -> Option<TsQueryStatus> {
    let line = trim_ts_line(raw_line).trim();
    if !line.starts_with("error ") && line != "error" {
        return None;
    }
    let mut id: Option<u32> = None;
    let mut msg = String::new();
    let mut extra_msg = None;

    for token in line.split_whitespace().skip(1) {
        if let Some((k, v)) = token.split_once('=') {
            match k {
                "id" => id = v.parse::<u32>().ok(),
                "msg" => msg = unescape_ts(v),
                "extra_msg" => extra_msg = Some(unescape_ts(v)),
                _ => {}
            }
        }
    }

    id.map(|id| TsQueryStatus { id, msg, extra_msg })
}

/// Parses pipe-separated ServerQuery response records into key/value maps.
pub fn parse_records(raw_line: &str) -> Vec<BTreeMap<String, String>> {
    let line = trim_ts_line(raw_line).trim();
    if line.is_empty() {
        return Vec::new();
    }

    line.split('|')
        .filter_map(|chunk| {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                return None;
            }
            let mut map = BTreeMap::new();
            for token in chunk.split(' ') {
                if token.is_empty() {
                    continue;
                }
                if let Some((k, v)) = token.split_once('=') {
                    map.insert(k.to_string(), unescape_ts(v));
                } else {
                    map.insert(token.to_string(), String::new());
                }
            }
            if map.is_empty() { None } else { Some(map) }
        })
        .collect()
}

/// Builds a ServerQuery command line terminated with `\n`.
pub fn build_command(
    cmd: &str,
    options: &[&str],
    params: &[(&str, &str)],
    batch_items: &[Vec<(&str, &str)>],
) -> String {
    let mut parts = Vec::new();
    parts.push(cmd.to_string());

    for (k, v) in params {
        parts.push(format!("{k}={}", escape_ts(v)));
    }

    if !batch_items.is_empty() {
        let mut encoded_items = Vec::with_capacity(batch_items.len());
        for item in batch_items {
            let kvs: Vec<String> = item
                .iter()
                .map(|(k, v)| format!("{k}={}", escape_ts(v)))
                .collect();
            encoded_items.push(kvs.join(" "));
        }
        parts.push(encoded_items.join("|"));
    }

    for opt in options {
        if opt.starts_with('-') {
            parts.push((*opt).to_string());
        } else {
            parts.push(format!("-{opt}"));
        }
    }

    let mut out = parts.join(" ");
    out.push('\n');
    out
}

/// Parses a single `notify*` asynchronous line emitted after `servernotifyregister`.
pub fn parse_notify_line(raw_line: &str) -> Option<TsNotifyEvent> {
    let line = trim_ts_line(raw_line).trim();
    if !line.starts_with("notify") {
        return None;
    }
    let (event_name, rest) = match line.split_once(' ') {
        Some((e, r)) => (e, r),
        None => (line, ""),
    };
    let recs = parse_records(rest);
    let rec = recs.into_iter().next().unwrap_or_default();

    let get_u32 = |k: &str| -> u32 { rec.get(k).and_then(|v| v.parse().ok()).unwrap_or(0) };
    let get_u8 = |k: &str| -> u8 { rec.get(k).and_then(|v| v.parse().ok()).unwrap_or(0) };
    let get_str = |k: &str| -> String { rec.get(k).cloned().unwrap_or_default() };

    match event_name {
        "notifycliententerview" => Some(TsNotifyEvent::ClientEntered {
            clid: get_u32("clid"),
            ctid: get_u32("ctid"),
            nickname: get_str("client_nickname"),
            uid: get_str("client_unique_identifier"),
        }),
        "notifyclientleftview" => Some(TsNotifyEvent::ClientLeft {
            clid: get_u32("clid"),
            cfid: get_u32("cfid"),
            reason_id: get_u32("reasonid"),
            reason_msg: get_str("reasonmsg"),
        }),
        "notifyclientmoved" => Some(TsNotifyEvent::ClientMoved {
            clid: get_u32("clid"),
            ctid: get_u32("ctid"),
            reason_id: get_u32("reasonid"),
            invoker_name: rec.get("invokername").cloned(),
        }),
        "notifychannelcreated" => Some(TsNotifyEvent::ChannelCreated {
            cid: get_u32("cid"),
            pid: get_u32("cpid"),
            name: get_str("channel_name"),
        }),
        "notifychanneledited" => Some(TsNotifyEvent::ChannelEdited {
            cid: get_u32("cid"),
            invoker_name: rec.get("invokername").cloned(),
        }),
        "notifychanneldeleted" => Some(TsNotifyEvent::ChannelDeleted {
            cid: get_u32("cid"),
        }),
        "notifychannelmoved" => Some(TsNotifyEvent::ChannelMoved {
            cid: get_u32("cid"),
            cpid: get_u32("cpid"),
            order: get_u32("order"),
        }),
        "notifytextmessage" => Some(TsNotifyEvent::TextMessage {
            target_mode: get_u8("targetmode"),
            invoker_id: get_u32("invokerid"),
            invoker_name: get_str("invokername"),
            msg: get_str("msg"),
        }),
        "notifyserveredited" => Some(TsNotifyEvent::ServerEdited),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_escaping() {
        let raw = "Apex [CN] / Squad | Alpha\\Beta\nLine2\r\t!";
        let esc = escape_ts(raw);
        assert_eq!(
            esc,
            "Apex\\s[CN]\\s\\/\\sSquad\\s\\p\\sAlpha\\\\Beta\\nLine2\\r\\t!"
        );
        assert_eq!(unescape_ts(&esc), raw);
    }

    #[test]
    fn parses_error_with_ts_lf_cr() {
        let status = parse_error_line("error id=2568 msg=insufficient\\sclient\\spermissions\n\r")
            .expect("should parse error");
        assert_eq!(status.id, 2568);
        assert_eq!(status.msg, "insufficient client permissions");
    }

    #[test]
    fn parses_piped_records() {
        let line = "cid=1 pid=0 channel_name=Lobby\\sRoom total_clients=2|cid=2 pid=1 channel_name=Sub\\pRoom total_clients=0\n\r";
        let recs = parse_records(line);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].get("channel_name").unwrap(), "Lobby Room");
        assert_eq!(recs[1].get("channel_name").unwrap(), "Sub|Room");
    }

    #[test]
    fn builds_command_with_options_and_escaping() {
        let cmd = build_command(
            "channellist",
            &["topic", "-flags", "limits"],
            &[("cid", "12"), ("channel_name", "My Channel")],
            &[],
        );
        assert_eq!(
            cmd,
            "channellist cid=12 channel_name=My\\sChannel -topic -flags -limits\n"
        );
    }

    #[test]
    fn parses_notify_client_moved() {
        let evt = parse_notify_line("notifyclientmoved ctid=5 reasonid=1 invokername=Admin clid=9\n\r")
            .expect("notify parsed");
        match evt {
            TsNotifyEvent::ClientMoved {
                clid,
                ctid,
                reason_id,
                invoker_name,
            } => {
                assert_eq!(clid, 9);
                assert_eq!(ctid, 5);
                assert_eq!(reason_id, 1);
                assert_eq!(invoker_name.as_deref(), Some("Admin"));
            }
            _ => panic!("wrong notify variant"),
        }
    }
}
