use ravet_osc::{Argument, OscError, OscSocket, Packet, decode, encode};
use std::{net::UdpSocket, time::Duration};
#[test]
fn independent_gain_packet_and_strict_truncated_prefixes() {
    let bytes = b"/gain\0\0\0,f\0\0\x40\0\0\0";
    let packet = Packet::Message {
        address: "/gain",
        arguments: vec![Argument::Float(2.)],
    };
    assert_eq!(encode(&packet).unwrap(), bytes);
    assert_eq!(decode(bytes).unwrap(), packet);
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length]).is_err(), "prefix {length}");
    }
    let mut padding = bytes.to_vec();
    padding[7] = 1;
    assert!(decode(&padding).is_err());
    let mut trailing = bytes.to_vec();
    trailing.extend([0; 4]);
    assert!(decode(&trailing).is_err());
    let mut unknown = bytes.to_vec();
    unknown[9] = b'x';
    assert!(matches!(
        decode(&unknown),
        Err(OscError::UnsupportedTag(b'x'))
    ));
}
#[test]
fn nested_bundles_and_optional_scalars_roundtrip() {
    let packet = Packet::Bundle {
        time_tag: 1,
        elements: vec![
            Packet::Message {
                address: "/all",
                arguments: vec![
                    Argument::Int(i32::MIN),
                    Argument::Long(i64::MAX),
                    Argument::Float(-0.),
                    Argument::Double(1.25),
                    Argument::String("hello"),
                    Argument::Blob(&[0, 255, 3]),
                    Argument::TimeTag(0x0102030405060708),
                    Argument::Bool(true),
                    Argument::Bool(false),
                    Argument::Nil,
                    Argument::Infinitum,
                ],
            },
            Packet::Bundle {
                time_tag: 0x0102030405060708,
                elements: vec![],
            },
        ],
    };
    let bytes = encode(&packet).unwrap();
    assert!(bytes.len().is_multiple_of(4));
    assert_eq!(decode(&bytes).unwrap(), packet);
    let mut blob = encode(&Packet::Message {
        address: "/b",
        arguments: vec![Argument::Blob(&[1, 2, 3])],
    })
    .unwrap();
    *blob.last_mut().unwrap() = 99;
    assert!(decode(&blob).is_err());
    blob[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode(&blob).is_err());
    assert!(
        encode(&Packet::Message {
            address: "bad",
            arguments: vec![]
        })
        .is_err()
    );
    assert!(
        encode(&Packet::Message {
            address: "/x",
            arguments: vec![Argument::String("音")]
        })
        .is_err()
    );
}
#[test]
fn byte_argument_count_packet_count_and_depth_limits() {
    let too_many = Packet::Message {
        address: "/x",
        arguments: vec![Argument::Nil; 257],
    };
    assert!(matches!(encode(&too_many), Err(OscError::Capacity)));
    let too_many = Packet::Bundle {
        time_tag: 1,
        elements: (0..256)
            .map(|_| Packet::Message {
                address: "/x",
                arguments: vec![],
            })
            .collect(),
    };
    assert!(matches!(encode(&too_many), Err(OscError::Capacity)));
    let mut bytes = encode(&Packet::Message {
        address: "/x",
        arguments: vec![],
    })
    .unwrap();
    for _ in 0..8 {
        let mut next = b"#bundle\0".to_vec();
        next.extend(1u64.to_be_bytes());
        next.extend((bytes.len() as u32).to_be_bytes());
        next.extend(bytes);
        bytes = next;
    }
    assert!(matches!(decode(&bytes), Err(OscError::Capacity)));
    assert!(matches!(decode(&vec![0; 65508]), Err(OscError::Capacity)));
    assert!(matches!(
        encode(&Packet::Message {
            address: "/x",
            arguments: vec![Argument::Blob(&vec![0; 65504])]
        }),
        Err(OscError::Capacity)
    ));
}
#[test]
fn explicit_loopback_transport_rejects_oversize_and_recovers() {
    let address = "127.0.0.1:0".parse().unwrap();
    let mut receiver = OscSocket::bind(address, 64, Duration::from_millis(50)).unwrap();
    let sender = OscSocket::bind(address, 64, Duration::from_millis(50)).unwrap();
    let packet = Packet::Message {
        address: "/gain",
        arguments: vec![Argument::Float(0.5)],
    };
    sender
        .send(receiver.local_address().unwrap(), &packet)
        .unwrap();
    let received = receiver.receive().unwrap();
    assert_eq!(received.sender, sender.local_address().unwrap());
    assert_eq!(received.packet, packet);
    let raw = UdpSocket::bind(address).unwrap();
    raw.send_to(&[0; 100], receiver.local_address().unwrap())
        .unwrap();
    assert!(matches!(receiver.receive(), Err(OscError::Capacity)));
    sender
        .send(receiver.local_address().unwrap(), &packet)
        .unwrap();
    assert_eq!(receiver.receive().unwrap().packet, packet);
    assert!(
        matches!(receiver.receive(),Err(OscError::Io(e)) if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut))
    );
}
