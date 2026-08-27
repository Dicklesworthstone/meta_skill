extern crate bincode_next as bincode;
use bincode::Decode;
use bincode::Encode;
use bincode::config;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use serde::Deserialize;
use serde::Serialize;
use std::hint::black_box;
use std::time::Duration;

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
struct World {
    players: Vec<Player>,
    map_metadata: MapMetadata,
    settings: Settings,
}

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
struct Player {
    id: u64,
    name: String,
    position: (f32, f32, f32),
    inventory: Vec<Item>,
    status: Status,
}

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
enum Item {
    Weapon { damage: u32, durability: f32 },
    Potion { heal_amount: u32, effect: String },
    Misc(String),
}

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
enum Status {
    Idle,
    Moving { speed: f32, destination: (f32, f32) },
    Attacking(u64),
}

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
struct MapMetadata {
    name: String,
    width: u32,
    height: u32,
    seed: u64,
}

#[derive(Serialize, Deserialize, Encode, Decode, PartialEq, Debug, Clone)]
struct Settings {
    difficulty: u8,
    max_players: u16,
    friendly_fire: bool,
}

fn create_mock_world() -> World {
    let mut players = Vec::new();
    for i in 0..100 {
        players.push(Player {
            id: i as u64,
            name: format!("Player {}", i),
            position: (i as f32, i as f32 * 1.5, i as f32 * 2.0),
            inventory: vec![
                Item::Weapon {
                    damage: 10 + i,
                    durability: 0.9,
                },
                Item::Potion {
                    heal_amount: 50,
                    effect: "Healing".to_string(),
                },
                Item::Misc("Key".to_string()),
            ],
            status: if i % 3 == 0 {
                Status::Idle
            } else if i % 3 == 1 {
                Status::Moving {
                    speed: 5.0,
                    destination: (100.0, 200.0),
                }
            } else {
                Status::Attacking(i as u64 - 1)
            },
        });
    }

    World {
        players,
        map_metadata: MapMetadata {
            name: "Epic World".to_string(),
            width: 1024,
            height: 1024,
            seed: 123456789,
        },
        settings: Settings {
            difficulty: 3,
            max_players: 100,
            friendly_fire: false,
        },
    }
}

fn bench_complex(c: &mut Criterion) {
    let world = create_mock_world();
    let config = config::standard();

    // bincode-next (Encode/Decode) - Using specialized traits
    let bytes_next = bincode::encode_to_vec(&world, config).unwrap();

    // bincode-v2 original (Serde) - We use Serde since World implements it and v2 recognizes it.
    let bytes_v2 =
        bincode_v2::serde::encode_to_vec(&world, bincode_v2::config::standard()).unwrap();

    // bincode-v2 original (Serde) - We use Serde since World implements it and v2 recognizes it.
    let bytes_v2_fixint =
        bincode_v2::serde::encode_to_vec(&world, bincode_v2::config::legacy()).unwrap();

    // bincode-v1 (Serde)
    let bytes_v1 = bincode_1::serialize(&world).unwrap();

    // CBOR format
    let config_cbor = config.with_cbor_format();
    let bytes_cbor = bincode::encode_to_vec(&world, config_cbor).unwrap();

    // Deterministic CBOR format
    let config_cbor_det = config.with_deterministic_cbor();
    let bytes_cbor_det = bincode::encode_to_vec(&world, config_cbor_det).unwrap();

    let mut group = c.benchmark_group("complex_world_decode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (traits, varint)", |b| {
        b.iter(|| {
            let res: (World, usize) =
                bincode::decode_from_slice(black_box(&bytes_next), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, fixed)", |b| {
        b.iter(|| {
            let res: (World, usize) =
                bincode::decode_from_slice(black_box(&bytes_v1), bincode::config::legacy())
                    .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (serde, varint)", |b| {
        b.iter(|| {
            let res: (World, usize) = bincode_v2::serde::decode_from_slice(
                black_box(&bytes_v2),
                bincode_v2::config::standard(),
            )
            .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (serde, fixed)", |b| {
        b.iter(|| {
            let res: (World, usize) = bincode_v2::serde::decode_from_slice(
                black_box(&bytes_v2_fixint),
                bincode_v2::config::legacy(),
            )
            .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v1 (serde)", |b| {
        b.iter(|| {
            let res: World = bincode_1::deserialize(black_box(&bytes_v1)).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, cbor)", |b| {
        b.iter(|| {
            let res: (World, usize) =
                bincode::decode_from_slice(black_box(&bytes_cbor), config_cbor).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, cbor-deterministic)", |b| {
        b.iter(|| {
            let res: (World, usize) =
                bincode::decode_from_slice(black_box(&bytes_cbor_det), config_cbor_det).unwrap();
            black_box(res);
        })
    });

    group.finish();

    let mut group = c.benchmark_group("complex_world_encode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (traits, varint)", |b| {
        b.iter(|| {
            let res = bincode::encode_to_vec(black_box(&world), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, fixed)", |b| {
        b.iter(|| {
            let res = bincode::encode_to_vec(black_box(&world), bincode::config::legacy()).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (serde, varint)", |b| {
        b.iter(|| {
            let res =
                bincode_v2::serde::encode_to_vec(black_box(&world), bincode_v2::config::standard())
                    .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (serde, fixed)", |b| {
        b.iter(|| {
            let res =
                bincode_v2::serde::encode_to_vec(black_box(&world), bincode_v2::config::legacy())
                    .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v1 (serde)", |b| {
        b.iter(|| {
            let res = bincode_1::serialize(black_box(&world)).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, cbor)", |b| {
        b.iter(|| {
            let res = bincode::encode_to_vec(black_box(&world), config_cbor).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-next (traits, cbor-deterministic)", |b| {
        b.iter(|| {
            let res = bincode::encode_to_vec(black_box(&world), config_cbor_det).unwrap();
            black_box(res);
        })
    });

    group.finish();
}

criterion_group!(complex, bench_complex);
criterion_main!(complex);
