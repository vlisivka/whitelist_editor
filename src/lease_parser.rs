use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lease {
    pub address: Option<String>,
    pub mac_address: String,
    pub client_id: Option<String>,
    pub server: String,
    pub comment: Option<String>,
    pub block_access: bool,
}

pub fn parse_leases(input: &str) -> Vec<Lease> {
    let mut leases = Vec::new();

    // Normalize input: join lines ending with backslash
    let normalized = input.replace("\\\n", "").replace("\\\r\n", "");

    // Each lease starts with "add "
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("add ") {
            if let Some(lease) = parse_line(trimmed) {
                leases.push(lease);
            }
        }
    }

    leases
}

fn parse_line(line: &str) -> Option<Lease> {
    let mut lease = Lease::default();

    // Regex to find key=value pairs.
    // Handles quoted values with escaped quotes.
    // keys: address, mac-address, client-id, server, comment, block-access
    let re = Regex::new(r#"(?P<key>[\w-]+)=\s*(?P<val>"[^"]*"|\S+)"#).unwrap();

    for cap in re.captures_iter(line) {
        let key = &cap["key"];
        let mut val = cap["val"].to_string();

        // Remove quotes if present
        if val.starts_with('"') && val.ends_with('"') {
            val = val[1..val.len() - 1].to_string();
        }

        match key {
            "address" => lease.address = Some(val),
            "mac-address" => lease.mac_address = val,
            "client-id" => lease.client_id = Some(val),
            "server" => lease.server = val,
            "comment" => lease.comment = Some(val),
            "block-access" => lease.block_access = val == "yes",
            _ => {} // Ignore unknown keys
        }
    }

    if lease.mac_address.is_empty() {
        dbg!(&line, &lease);
        None
    } else {
        Some(lease)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_example() {
        let input = r#"add address=192.168.20.119 block-access=yes client-id=1:d0:57:7e:a0:bb:99 mac-address=D0:57:7E:AA:BB:99 server=corp-dhcp
add address=192.168.20.169 comment="An IPhone" mac-address=40:9C:28:AA:39:6F server=corp-dhcp
add address=192.168.20.121 client-id=1:f8:ed:fc:dd:b5:f1 comment="!!!\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD, \EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD\EF\BF\BD, HP ProBook 460 G11" \
    mac-address=F8:ED:FC:AA:B5:F1 server=corp-dhcp"#;

        let leases = parse_leases(input);
        assert_eq!(leases.len(), 3);

        assert_eq!(leases[0].address.as_deref(), Some("192.168.20.119"));
        assert_eq!(leases[0].block_access, true);
        assert_eq!(leases[0].mac_address, "D0:57:7E:AA:BB:99");
        assert_eq!(leases[0].server, "corp-dhcp");

        assert_eq!(leases[1].address.as_deref(), Some("192.168.20.169"));
        assert_eq!(leases[1].mac_address, "40:9C:28:AA:39:6F");
        assert_eq!(leases[1].server, "corp-dhcp");

        assert_eq!(leases[2].address.as_deref(), Some("192.168.20.121"));
        assert_eq!(leases[2].mac_address, "F8:ED:FC:AA:B5:F1");
        assert_eq!(leases[2].server, "corp-dhcp");
        assert!(
            leases[2]
                .comment
                .as_ref()
                .unwrap()
                .contains("HP ProBook 460 G11")
        );
    }

    #[test]
    fn test_parse_padded() {
        let input = r#"add address=192.168.21.153 comment="Noutbuk UU" mac-address=    08:97:98:EE:36:9B server=corp-dhcp"#;
        let leases = parse_leases(input);
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].address.as_deref(), Some("192.168.21.153"));
        assert_eq!(leases[0].mac_address, "08:97:98:EE:36:9B");
        assert_eq!(leases[0].comment.as_deref(), Some("Noutbuk UU"));
    }
}
