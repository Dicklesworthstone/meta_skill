# Bench Results of bincode-next

This is the Bench Results of bincode-next v3.0.0-rc.1.

## Bench Environment

```shell
git clone https://github.com/Apich-Organization/bincode.git
cd bincode
cargo bench --bench extreme_perf
cargo bench --bench complex
```

---

```plaintext
Operating System: Fedora Linux 43
KDE Plasma Version: 6.6.0
KDE Frameworks Version: 6.23.0
Qt Version: 6.10.1
Kernel Version: 6.18.9-200.fc43.x86_64 (64-bit)
Graphics Platform: Wayland
Processors: 8 × Intel® Core™ i7-8665U CPU @ 1.90GHz
Memory: 32 GiB of RAM (31.0 GiB usable)
Graphics Processor: Intel® UHD Graphics 620
Manufacturer: Dell Inc.
Product Name: Latitude 5400
```

## Real World Bench Example

```shell
# We need root permission and use nightly compiler to ensure the most accurate result
sudo cargo +nightly bench --bench complex
```

---

```plaintext
complex_world_decode/bincode-next (traits, varint)
                        time:   [16.819 µs 16.878 µs 16.950 µs]
                        change: [−14.387% −12.575% −10.995%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  3 (3.00%) low mild
  4 (4.00%) high mild
  8 (8.00%) high severe
complex_world_decode/bincode-next (traits, fixed)
                        time:   [21.782 µs 21.872 µs 22.009 µs]
                        change: [−10.289% −9.7849% −9.2180%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  1 (1.00%) high mild
  5 (5.00%) high severe
complex_world_decode/bincode-v2 (serde, varint)
                        time:   [25.637 µs 25.727 µs 25.820 µs]
                        change: [−4.9126% −4.5062% −4.1169%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
complex_world_decode/bincode-v2 (serde, fixed)
                        time:   [21.920 µs 21.973 µs 22.037 µs]
                        change: [−4.1017% −3.6962% −3.2912%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
complex_world_decode/bincode-v1 (serde)
                        time:   [22.048 µs 22.074 µs 22.105 µs]
                        change: [−16.582% −15.877% −15.149%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 12 outliers among 100 measurements (12.00%)
  5 (5.00%) high mild
  7 (7.00%) high severe

complex_world_encode/bincode-next (traits, varint)
                        time:   [3.3416 µs 3.3467 µs 3.3526 µs]
                        change: [−9.4844% −8.9459% −8.2847%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 11 outliers among 100 measurements (11.00%)
  8 (8.00%) high mild
  3 (3.00%) high severe
complex_world_encode/bincode-next (traits, fixed)
                        time:   [2.9236 µs 2.9350 µs 2.9527 µs]
                        change: [−7.0516% −6.4181% −5.8909%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
complex_world_encode/bincode-v2 (serde, varint)
                        time:   [4.2454 µs 4.2489 µs 4.2525 µs]
                        change: [−9.7050% −9.2041% −8.8004%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  1 (1.00%) low severe
  4 (4.00%) low mild
  5 (5.00%) high mild
  3 (3.00%) high severe
complex_world_encode/bincode-v2 (serde, fixed)
                        time:   [3.3257 µs 3.3295 µs 3.3337 µs]
                        change: [−4.6908% −4.2786% −3.9410%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  2 (2.00%) low mild
  1 (1.00%) high mild
  3 (3.00%) high severe
complex_world_encode/bincode-v1 (serde)
                        time:   [3.0716 µs 3.0767 µs 3.0823 µs]
                        change: [−6.0147% −5.6804% −5.3628%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe
```

### **Performance Comparison: Decoding**

*Baseline: **bincode-next (traits, varint)** at 16.878 µs*

| Rank | Implementation | Interface | Int Encoding | Median Time | Relative Speed |
| --- | --- | --- | --- | --- | --- |
| **1** | **bincode-next** | traits | varint | **16.878 µs** | **1.00x** |
| 2 | **bincode-next** | traits | fixed | 21.872 µs | 1.30x |
| 3 | **bincode-v2** | serde | fixed | 21.973 µs | 1.30x |
| 4 | **bincode-v1** | serde | N/A | 22.074 µs | 1.31x |
| 5 | **bincode-v2** | serde | varint | 25.727 µs | 1.52x |

---

### **Performance Comparison: Encoding**

*Baseline: **bincode-next (traits, fixed)** at 2.9350 µs*

| Rank | Implementation | Interface | Int Encoding | Median Time | Relative Speed |
| --- | --- | --- | --- | --- | --- |
| **1** | **bincode-next** | traits | fixed | **2.9350 µs** | **1.00x** |
| 2 | **bincode-v1** | serde | N/A | 3.0767 µs | 1.05x |
| 3 | **bincode-v2** | serde | fixed | 3.3295 µs | 1.13x |
| 4 | **bincode-next** | traits | varint | 3.3467 µs | 1.14x |
| 5 | **bincode-v2** | serde | varint | 4.2489 µs | 1.45x |

---

### **Efficiency Score: Combined Round-Trip Performance**

*Sum of Median Decode + Median Encode (Normalized to the fastest = 1.00x)*

| Rank | Implementation | Interface | Int Encoding | Total Time | Efficiency Score |
| --- | --- | --- | --- | --- | --- |
| **1** | **bincode-next** | **traits** | **varint** | **20.225 µs** | **1.00x** |
| 2 | **bincode-next** | traits | fixed | 24.807 µs | 1.23x |
| 3 | **bincode-v1** | serde | N/A | 25.151 µs | 1.24x |
| 4 | **bincode-v2** | serde | fixed | 25.303 µs | 1.25x |
| 5 | **bincode-v2** | serde | varint | 29.976 µs | 1.48x |

## Theoretical Performance Bench Example

```shell
# We need root permission and use nightly compiler to ensure the most accurate result
sudo cargo +nightly bench --bench extreme_perf
```

---

```plaintext
vec_u64_small_varint_decode/bincode-next (current)
                        time:   [2.8170 µs 2.8256 µs 2.8365 µs]
                        change: [−17.956% −16.784% −15.682%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  2 (2.00%) high mild
  11 (11.00%) high severe
vec_u64_small_varint_decode/bincode-v2 (original)
                        time:   [12.411 µs 12.450 µs 12.493 µs]
                        change: [−15.187% −14.115% −13.189%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 8 outliers among 100 measurements (8.00%)
  1 (1.00%) high mild
  7 (7.00%) high severe

vec_u64_large_varint_decode/bincode-next (current)
                        time:   [12.991 µs 13.062 µs 13.154 µs]
                        change: [−10.932% −9.9467% −9.0857%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 9 outliers among 100 measurements (9.00%)
  4 (4.00%) high mild
  5 (5.00%) high severe
vec_u64_large_varint_decode/bincode-v2 (original)
                        time:   [17.398 µs 17.635 µs 17.898 µs]
                        change: [−3.7287% −2.8902% −1.8419%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe

vec_u64_fixint_native_decode/bincode-next (current)
                        time:   [1.8350 µs 1.8373 µs 1.8396 µs]
                        change: [−6.3092% −6.1285% −5.9622%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 11 outliers among 100 measurements (11.00%)
  2 (2.00%) low severe
  5 (5.00%) low mild
  1 (1.00%) high mild
  3 (3.00%) high severe
vec_u64_fixint_native_decode/bincode-v2 (original)
                        time:   [10.104 µs 10.129 µs 10.170 µs]
                        change: [−4.6863% −4.3429% −4.0001%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) low mild
  3 (3.00%) high severe
vec_u64_fixint_native_decode/bincode-v1
                        time:   [7.5019 µs 7.5378 µs 7.5711 µs]
                        change: [+1.8251% +2.1775% +2.5445%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

vec_u8_bulk_decode/bincode-next (current)
                        time:   [160.22 ns 160.44 ns 160.67 ns]
                        change: [−2.5803% −2.3878% −2.1975%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
vec_u8_bulk_decode/bincode-v2 (original)
                        time:   [273.32 ns 273.86 ns 274.50 ns]
                        change: [−4.2012% −3.7499% −3.3633%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  5 (5.00%) low mild
  2 (2.00%) high mild
  7 (7.00%) high severe
vec_u8_bulk_decode/bincode-v1
                        time:   [6.2752 µs 6.3070 µs 6.3441 µs]
                        change: [−13.013% −12.540% −11.983%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
```

### **1. Vector `u64` Decoding: Varint Performance**

*Contrasting small vs. large integer varint decoding.*

| Dataset | Implementation | Median Time | Relative Speed |
| --- | --- | --- | --- |
| **Small Varint** | **bincode-next (current)** | **2.8256 µs** | **1.00x** |
| Small Varint | bincode-v2 (original) | 12.450 µs | 4.41x |
|  |  |  |  |
| **Large Varint** | **bincode-next (current)** | **13.062 µs** | **1.00x** |
| Large Varint | bincode-v2 (original) | 17.635 µs | 1.35x |

---

### **2. Vector `u64` Decoding: Fixed Performance**

*Baseline: **bincode-next (current)** at 1.8373 µs*

| Rank | Implementation | Median Time | Relative Speed |
| --- | --- | --- | --- |
| **1** | **bincode-next (current)** | **1.8373 µs** | **1.00x** |
| 2 | bincode-v1 | 7.5378 µs | 4.10x |
| 3 | bincode-v2 (original) | 10.129 µs | 5.51x |

---

### **3. Bulk `u8` Decoding: Throughput Performance**

*Baseline: **bincode-next (current)** at 160.44 ns*

| Rank | Implementation | Median Time | Relative Speed |
| --- | --- | --- | --- |
| **1** | **bincode-next (current)** | **160.44 ns** | **1.00x** |
| 2 | bincode-v2 (original) | 273.86 ns | 1.71x |
| 3 | bincode-v1 | 6307.00 ns | 39.31x |

## Links

[Criterion.rs Benchmark Index](report/index.html)
