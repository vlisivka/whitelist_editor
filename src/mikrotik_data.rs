use ipnet::Ipv4Net;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Lease {
    pub address: Option<String>,
    pub mac_address: String,
    pub client_id: Option<String>,
    pub server: String,
    pub comment: Option<String>,
    pub block_access: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DhcpServer {
    pub name: String,
    pub comment: Option<String>,
    pub interface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DhcpNetwork {
    pub address: String, // CIDR
    pub comment: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DhcpData {
    pub servers: Vec<DhcpServer>,
    pub networks: Vec<DhcpNetwork>,
    pub leases: Vec<Lease>,
}

pub fn parse_all(input: &str) -> DhcpData {
    let mut data = DhcpData::default();
    let normalized = input.replace("\\\n", "").replace("\\\r\n", "");
    let mut current_section = "";

    let re_kv = Regex::new(r#"(?P<key>[\w-]+)=\s*(?P<val>"(?:[^"\\]|\\.)*"|\S+)"#).unwrap();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.to_lowercase() == "/ip dhcp-server" {
            current_section = "server";
            continue;
        } else if trimmed.to_lowercase() == "/ip dhcp-server lease" {
            current_section = "lease";
            continue;
        } else if trimmed.to_lowercase() == "/ip dhcp-server network" {
            current_section = "network";
            continue;
        } else if trimmed.starts_with('/') {
            current_section = "";
            continue;
        }

        if trimmed.starts_with("add ") {
            match current_section {
                "server" => {
                    let mut s = DhcpServer::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                            val = unescape_mikrotik(&val);
                        } else if val.contains('\\') {
                            val = unescape_mikrotik(&val);
                        }
                        match key {
                            "name" => s.name = val,
                            "comment" => s.comment = Some(val),
                            "interface" => s.interface = val,
                            _ => {}
                        }
                    }
                    if !s.name.is_empty() {
                        data.servers.push(s);
                    }
                }
                "lease" => {
                    let mut l = Lease::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                            val = unescape_mikrotik(&val);
                        } else if val.contains('\\') {
                            val = unescape_mikrotik(&val);
                        }
                        match key {
                            "address" => l.address = Some(val),
                            "mac-address" => {
                                // Sometimes mac-address has extra spaces in export
                                l.mac_address = val.trim().to_string();
                            }
                            "client-id" => l.client_id = Some(val),
                            "server" => l.server = val,
                            "comment" => l.comment = Some(val),
                            "block-access" => l.block_access = val == "yes",
                            _ => {}
                        }
                    }
                    if !l.mac_address.is_empty() {
                        data.leases.push(l);
                    }
                }
                "network" => {
                    let mut n = DhcpNetwork::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                            val = unescape_mikrotik(&val);
                        } else if val.contains('\\') {
                            val = unescape_mikrotik(&val);
                        }
                        match key {
                            "address" => n.address = val,
                            "comment" => n.comment = Some(val),
                            "gateway" => n.gateway = Some(val),
                            _ => {}
                        }
                    }
                    if !n.address.is_empty() {
                        data.networks.push(n);
                    }
                }
                _ => {}
            }
        }
    }
    data
}

pub fn find_network_for_server<'a>(
    server: &DhcpServer,
    networks: &'a [DhcpNetwork],
) -> Option<&'a DhcpNetwork> {
    // Priority 1: Match by comment
    if let Some(net) = server.comment.as_ref().and_then(|s_comment| {
        networks
            .iter()
            .find(|n| n.comment.as_ref() == Some(s_comment))
    }) {
        return Some(net);
    }

    // Priority 2: Fallback (can't really match by interface easily without more data)
    // In many cases, people use the same name or comment.
    // If no comment, maybe try matching server name with network comment?
    if let Some(net) = networks
        .iter()
        .find(|n| n.comment.as_ref() == Some(&server.name))
    {
        return Some(net);
    }

    None
}

pub fn is_ip_in_range(ip: &str, network: &DhcpNetwork) -> bool {
    let ip_parsed: Ipv4Addr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    let net_parsed: Ipv4Net = match network.address.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    net_parsed.contains(&ip_parsed)
}

pub fn find_first_free_ip(network: &DhcpNetwork, existing_leases: &[Lease]) -> Option<String> {
    let net_parsed: Ipv4Net = network.address.parse().ok()?;
    let gateway: Option<Ipv4Addr> = network.gateway.as_ref().and_then(|g| g.parse().ok());

    let mut taken_ips: std::collections::HashSet<Ipv4Addr> = existing_leases
        .iter()
        .filter_map(|l| l.address.as_ref()?.parse().ok())
        .collect();

    if let Some(g) = gateway {
        taken_ips.insert(g);
    }

    // ipnet hosts() returns usable hosts (skips network/broadcast)
    for ip in net_parsed.hosts() {
        if !taken_ips.contains(&ip) {
            return Some(ip.to_string());
        }
    }
    None
}

pub fn is_ip_unique(ip: &str, existing_leases: &[Lease], current_mac: &str) -> bool {
    let ip_val = ip.trim();
    if ip_val.is_empty() {
        return true;
    }

    !existing_leases
        .iter()
        .any(|l| l.address.as_deref() == Some(ip_val) && l.mac_address != current_mac)
}

pub fn is_valid_mac(mac: &str) -> bool {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^([0-9A-Fa-f]{2}:){5}([0-9A-Fa-f]{2})$").unwrap());
    re.is_match(mac)
}

pub fn unescape_mikrotik(input: &str) -> String {
    let mut result = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    '\\' | '\"' | '\'' | '?' | '$' | '_' => {
                        let mapped = match next {
                            '_' => ' ',
                            _ => next,
                        };
                        result.extend_from_slice(mapped.to_string().as_bytes());
                        chars.next();
                    }
                    'n' => {
                        result.push(b'\n');
                        chars.next();
                    }
                    'r' => {
                        result.push(b'\r');
                        chars.next();
                    }
                    't' => {
                        result.push(b'\t');
                        chars.next();
                    }
                    'a' => {
                        result.push(0x07);
                        chars.next();
                    }
                    'b' => {
                        result.push(0x08);
                        chars.next();
                    }
                    'f' => {
                        result.push(0x0c);
                        chars.next();
                    }
                    'v' => {
                        result.push(0x0b);
                        chars.next();
                    }
                    _ if next.is_ascii_hexdigit() => {
                        // Hex escape \HH
                        let h1 = chars.next().unwrap();
                        if let Some(&h2) = chars.peek() {
                            if h2.is_ascii_hexdigit() {
                                chars.next();
                                let hex = format!("{}{}", h1, h2);
                                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                    result.push(byte);
                                }
                            } else {
                                // Just one digit? MikroTik usually uses 2.
                                // If not 2 digits, we might just treat it as literal or error.
                                // But let's assume 2 digits.
                            }
                        }
                    }
                    _ => {
                        // Unknown escape, keep backslash?
                        result.push(b'\\');
                    }
                }
            } else {
                result.push(b'\\');
            }
        } else {
            result.extend_from_slice(c.to_string().as_bytes());
        }
    }

    String::from_utf8_lossy(&result).into_owned()
}

pub fn escape_mikrotik(input: &str) -> String {
    let mut result = String::from("\"");
    for c in input.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\"' => result.push_str("\\\""),
            '$' => result.push_str("\\$"),
            '?' => result.push_str("\\?"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ if c.is_ascii() && !c.is_ascii_control() => result.push(c),
            _ => {
                // Unicode or control char -> hex escapes
                let mut buf = [0; 4];
                for &byte in c.encode_utf8(&mut buf).as_bytes() {
                    result.push_str(&format!("\\{:02X}", byte));
                }
            }
        }
    }
    result.push('\"');
    result
}

pub fn is_valid_ipv4(
    ip: &str,
    network: &DhcpNetwork,
    existing_leases: &[Lease],
    current_mac: &str,
) -> bool {
    let ip_val = ip.trim();
    if ip_val.is_empty() {
        return false;
    }

    // 1. Format check
    if ip_val.parse::<Ipv4Addr>().is_err() {
        return false;
    }

    // 2. Range check
    if !is_ip_in_range(ip_val, network) {
        return false;
    }

    // 3. Uniqueness check
    if !is_ip_unique(ip_val, existing_leases, current_mac) {
        return false;
    }

    true
}

/// Стовпець, по якому виконується сортування таблиці лізів.
#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Ip,
    Mac,
    Server,
    Comment,
}

/// Порядок сортування.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// Сортує `leases` за заданим стовпцем та порядком.
/// Якщо `column` — `None`, повертає вхідний список без змін.
/// `None`-значення полів (comment, address) розміщуються в кінці при Asc
/// та на початку при Desc.
pub fn sort_leases<'a>(
    leases: Vec<&'a Lease>,
    column: Option<&SortColumn>,
    order: &SortOrder,
) -> Vec<&'a Lease> {
    let Some(col) = column else {
        return leases;
    };

    let mut sorted = leases;
    sorted.sort_by(|a, b| {
        let cmp = match col {
            SortColumn::Ip => {
                // Числове сортування: 10.0.0.9 < 10.0.0.10
                let a_ip: Option<Ipv4Addr> = a.address.as_deref().and_then(|s| s.parse().ok());
                let b_ip: Option<Ipv4Addr> = b.address.as_deref().and_then(|s| s.parse().ok());
                match (a_ip, b_ip) {
                    (Some(a), Some(b)) => u32::from(a).cmp(&u32::from(b)),
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            }
            SortColumn::Mac => a
                .mac_address
                .to_lowercase()
                .cmp(&b.mac_address.to_lowercase()),
            SortColumn::Server => a.server.to_lowercase().cmp(&b.server.to_lowercase()),
            SortColumn::Comment => {
                let a_c = a.comment.as_deref().unwrap_or("");
                let b_c = b.comment.as_deref().unwrap_or("");
                match (a.comment.is_some(), b.comment.is_some()) {
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, false) => std::cmp::Ordering::Less,
                    _ => a_c.to_lowercase().cmp(&b_c.to_lowercase()),
                }
            }
        };
        if *order == SortOrder::Desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
    sorted
}

/// Повертає підмножину лізів, у яких хоча б одне поле
/// містить рядок `query` (нечутливо до регістру).
/// Порожній `query` → повертає всі ліза.
pub fn filter_leases<'a>(leases: &'a [Lease], query: &str) -> Vec<&'a Lease> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return leases.iter().collect();
    }
    leases
        .iter()
        .filter(|l| {
            l.address
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&q)
                || l.mac_address.to_lowercase().contains(&q)
                || l.server.to_lowercase().contains(&q)
                || l.comment
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIKROTIK_EXPORT: &str = r#"
/ip dhcp-server
add add-arp=yes comment=guest interface=GSTVLAN name=guest-dhcp
add add-arp=yes comment=corp interface=CRPVLAN lease-time=1h name=corp-dhcp
add add-arp=yes comment=manage interface=MNGVLAN name=mng-server
/ip dhcp-server lease
add address=172.16.20.217 block-access=yes mac-address=A4:C6:9A:08:86:C8 server=corp-dhcp
add address=172.22.2.29 comment=029SYN mac-address=F4:1E:57:7F:D1:57 server=mng-server
/ip dhcp-server network
add address=172.16.20.0/23 comment=corp dns-server=172.16.20.1 gateway=172.16.20.1
add address=172.22.2.0/24 comment=manage dns-server=172.22.2.2 gateway=172.22.2.1
add address=192.168.10.0/24 comment=guest dns-server=192.168.10.1 gateway=192.168.10.1
"#;

    #[test]
    fn test_parse_all() {
        let data = parse_all(MIKROTIK_EXPORT);

        assert_eq!(data.servers.len(), 3);
        assert_eq!(data.servers[0].name, "guest-dhcp");
        assert_eq!(data.servers[0].comment.as_deref(), Some("guest"));

        assert_eq!(data.leases.len(), 2);
        assert_eq!(data.leases[0].address.as_deref(), Some("172.16.20.217"));
        assert_eq!(data.leases[0].server, "corp-dhcp");

        assert_eq!(data.networks.len(), 3);
        assert_eq!(data.networks[0].address, "172.16.20.0/23");
        assert_eq!(data.networks[0].comment.as_deref(), Some("corp"));
    }

    #[test]
    fn test_match_server_network() {
        let data = parse_all(MIKROTIK_EXPORT);
        let corp_server = data.servers.iter().find(|s| s.name == "corp-dhcp").unwrap();

        let matched = find_network_for_server(corp_server, &data.networks);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().address, "172.16.20.0/23");
    }

    #[test]
    fn test_ip_in_range() {
        let net = DhcpNetwork {
            address: "172.16.20.0/23".into(),
            ..Default::default()
        };
        assert!(is_ip_in_range("172.16.20.5", &net));
        assert!(is_ip_in_range("172.16.21.254", &net));
        assert!(!is_ip_in_range("172.16.22.1", &net));
    }

    #[test]
    fn test_find_first_free_ip() {
        let net = DhcpNetwork {
            address: "172.22.2.0/24".into(),
            gateway: Some("172.22.2.1".into()),
            ..Default::default()
        };
        let existing = vec![
            Lease {
                address: Some("172.22.2.2".to_string()),
                ..Default::default()
            },
            Lease {
                address: Some("172.22.2.3".to_string()),
                ..Default::default()
            },
        ];
        // Should skip .0 (network), .1 (gateway), .2 (existing), .3 (existing)
        // So .4 should be free
        let free = find_first_free_ip(&net, &existing);
        assert_eq!(free.as_deref(), Some("172.22.2.4"));
    }

    #[test]
    fn test_is_valid_mac() {
        assert!(is_valid_mac("A4:C6:9A:08:86:C8"));
        assert!(is_valid_mac("a4:c6:9a:08:86:c8"));
        assert!(!is_valid_mac("A4:C6:9A:08:86:G8")); // Invalid char
        assert!(!is_valid_mac("A4-C6-9A-08-86-C8")); // Wrong separator
        assert!(!is_valid_mac("A4:C6:9A:08:86")); // Too short
    }

    #[test]
    fn test_is_valid_ipv4() {
        let net = DhcpNetwork {
            address: "172.16.20.0/23".into(),
            ..Default::default()
        };
        let existing = vec![Lease {
            address: Some("172.16.20.10".to_string()),
            mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
            ..Default::default()
        }];

        // Valid
        assert!(is_valid_ipv4(
            "172.16.20.50",
            &net,
            &existing,
            "00:11:22:33:44:55"
        ));
        // Invalid format
        assert!(!is_valid_ipv4(
            "172.16.20.256",
            &net,
            &existing,
            "00:11:22:33:44:55"
        ));
        // Out of range
        assert!(!is_valid_ipv4(
            "172.16.22.1",
            &net,
            &existing,
            "00:11:22:33:44:55"
        ));
        // Not unique (another MAC already has this IP)
        assert!(!is_valid_ipv4(
            "172.16.20.10",
            &net,
            &existing,
            "00:11:22:33:44:55"
        ));
        // Unique (same MAC - editing own lease)
        assert!(is_valid_ipv4(
            "172.16.20.10",
            &net,
            &existing,
            "AA:BB:CC:DD:EE:FF"
        ));
    }

    #[test]
    fn test_unescape_mikrotik() {
        assert_eq!(
            super::unescape_mikrotik(r#"\"Hello world\""#),
            r#""Hello world""#
        );
        assert_eq!(super::unescape_mikrotik(r#"Hello\_world"#), "Hello world");
        assert_eq!(super::unescape_mikrotik(r#"\D0\B0\D0\B1\D1\82"#), "абт");
        assert_eq!(super::unescape_mikrotik(r#"\?\$"#), "?$");
        assert_eq!(super::unescape_mikrotik(r#"a\nb"#), "a\nb");
    }

    #[test]
    fn test_escape_mikrotik() {
        assert_eq!(
            super::escape_mikrotik("Привіт"),
            r#""\D0\9F\D1\80\D0\B8\D0\B2\D1\96\D1\82""#
        );
        assert_eq!(
            super::escape_mikrotik(r#"Path with "quotes""#),
            r#""Path with \"quotes\"""#
        );
        assert_eq!(super::escape_mikrotik("simple"), r#""simple""#);
    }

    // --- filter_leases ---

    fn make_leases() -> Vec<Lease> {
        vec![
            Lease {
                address: Some("172.16.20.217".to_string()),
                mac_address: "A4:C6:9A:08:86:C8".to_string(),
                server: "corp-dhcp".to_string(),
                comment: None,
                block_access: true,
                client_id: None,
            },
            Lease {
                address: Some("172.22.2.29".to_string()),
                mac_address: "F4:1E:57:7F:D1:57".to_string(),
                server: "mng-server".to_string(),
                comment: Some("029SYN".to_string()),
                block_access: false,
                client_id: None,
            },
        ]
    }

    #[test]
    fn test_filter_leases_empty_query() {
        let leases = make_leases();
        let result = filter_leases(&leases, "");
        assert_eq!(result.len(), 2, "Порожній запит має повертати всі записи");
    }

    #[test]
    fn test_filter_leases_empty_query_whitespace() {
        let leases = make_leases();
        let result = filter_leases(&leases, "   ");
        assert_eq!(result.len(), 2, "Запит з пробілів має повертати всі записи");
    }

    #[test]
    fn test_filter_leases_by_ip() {
        let leases = make_leases();
        let result = filter_leases(&leases, "172.22");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address.as_deref(), Some("172.22.2.29"));
    }

    #[test]
    fn test_filter_leases_by_mac() {
        let leases = make_leases();
        let result = filter_leases(&leases, "f4:1e:57");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mac_address, "F4:1E:57:7F:D1:57");
    }

    #[test]
    fn test_filter_leases_by_server() {
        let leases = make_leases();
        let result = filter_leases(&leases, "corp-dhcp");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].server, "corp-dhcp");
    }

    #[test]
    fn test_filter_leases_by_comment() {
        let leases = make_leases();
        let result = filter_leases(&leases, "029syn");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].comment.as_deref(), Some("029SYN"));
    }

    #[test]
    fn test_filter_leases_no_match() {
        let leases = make_leases();
        let result = filter_leases(&leases, "xxxxxxx");
        assert!(
            result.is_empty(),
            "Немає збігів — має повертати порожній вектор"
        );
    }

    #[test]
    fn test_filter_leases_case_insensitive() {
        let leases = make_leases();
        // «SYN» має знайти коментар «029SYN»
        let result = filter_leases(&leases, "SYN");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].comment.as_deref(), Some("029SYN"));
    }

    // --- sort_leases ---

    #[test]
    fn test_sort_leases_no_sort() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, None, &SortOrder::Asc);
        // Порядок має залишитися незмінним
        assert_eq!(result[0].address.as_deref(), Some("172.16.20.217"));
        assert_eq!(result[1].address.as_deref(), Some("172.22.2.29"));
    }

    #[test]
    fn test_sort_leases_by_ip_asc() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, Some(&SortColumn::Ip), &SortOrder::Asc);
        // 172.16.20.217 < 172.22.2.29 числово
        assert_eq!(result[0].address.as_deref(), Some("172.16.20.217"));
        assert_eq!(result[1].address.as_deref(), Some("172.22.2.29"));
    }

    #[test]
    fn test_sort_leases_by_ip_desc() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, Some(&SortColumn::Ip), &SortOrder::Desc);
        // Спадання: 172.22.x перший
        assert_eq!(result[0].address.as_deref(), Some("172.22.2.29"));
        assert_eq!(result[1].address.as_deref(), Some("172.16.20.217"));
    }

    #[test]
    fn test_sort_leases_by_mac_asc() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, Some(&SortColumn::Mac), &SortOrder::Asc);
        // "A4:..." < "F4:..." лексикографічно
        assert_eq!(result[0].mac_address, "A4:C6:9A:08:86:C8");
        assert_eq!(result[1].mac_address, "F4:1E:57:7F:D1:57");
    }

    #[test]
    fn test_sort_leases_by_server_asc() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, Some(&SortColumn::Server), &SortOrder::Asc);
        // "corp-dhcp" < "mng-server"
        assert_eq!(result[0].server, "corp-dhcp");
        assert_eq!(result[1].server, "mng-server");
    }

    #[test]
    fn test_sort_leases_by_comment_asc() {
        let leases = make_leases();
        let refs: Vec<&Lease> = leases.iter().collect();
        let result = sort_leases(refs, Some(&SortColumn::Comment), &SortOrder::Asc);
        // Lease з comment=Some("029SYN") < Lease з comment=None (None — в кінці)
        assert_eq!(result[0].comment.as_deref(), Some("029SYN"));
        assert!(result[1].comment.is_none());
    }
}
