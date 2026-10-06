# Reset 909 NMF-resynthesized sources

Three fixed PCM sources serve model IDs 24–27: one source shared by closed/open hat, one crash and
one ride. On 2026-09-18 the owner replaced the earlier Medium all-pass transforms with the
listening-selected NMF-12 results from private comparisons. The comparison recordings and generated
WAV intermediates remain external; only these signed 16-bit little-endian mono sources and the
project-generated closed-envelope table are embedded.

All analysis used a 512-sample Hann STFT with a 64-sample hop at 44.1 kHz and twelve-component
nonnegative matrix factorization for 300 multiplicative-update iterations.

- **Shared hat:** generalized Kullback–Leibler NMF soft masks separate twelve coherent source
  components. Each mask multiplies the source complex STFT, each component is inverted once,
  converted to an analytic signal with a 1,024-sample edge pad, given one constant phase rotation,
  and summed. Factor seed 9302, phase seed 9401, and listening-selected rotation depth 0.45 times
  angles drawn uniformly from `[-pi, pi]`. A 0.75 dB long-path envelope correction enters from
  45–110 ms, holds through the audible body, and returns to unity from 300–430 ms.
- **Closed articulation:** no second PCM source exists. Both hats restart `hat.pcm`; the closed path
  applies `closed-envelope-f32.table`, a 13,920-frame gain curve measured as the 2 ms RMS-envelope
  ratio between the closed comparison and the shared source, smoothed for 4 ms in log gain and
  normalized to unity. A deterministic source-addressed white residual at listening-selected amount
  0.03 follows the same curve; it restores the closed recording's less metallic body without adding
  a host-rate random process. The earlier separately generated closed KL result remains measurement
  evidence, not a shipped source.
- **Crash and ride:** the same coherent-component KL method, seeds, and 0.45 rotation depth, without
  the hat envelope correction. Crash uses the comparison set's `Full Decay` capture; ride uses its
  `Max Decay` capture. Other settings are DSP envelope deviations rather than separate embedded
  sources.

Direct long-tail NMF magnitude inversion was rejected because inconsistent overlap-add sounded like
a very short feedback delay. Generic modal/noise reductions were rejected as washy; wider component
rotation was clearly worse. The accepted coherent-component method has no recursive delay,
all-pass, runtime FFT, or Griffin–Lim in the shipped path: all factorization is offline and the DSP
plays these fixed results.

The transforms remain source-derived and retain comparison-recording information; listening found
the selected shared hat and crash results effectively indistinguishable from their references. **The
comparison recordings are the owner's own recordings of a TR-909** (the owner, 2026-10-06), so these
sources may be published and sold with the plugin; the recordings themselves stay outside this
repository.

The generated WAVs were RMS-preserved against their comparison input, jointly peak-scaled with that
input for audition, and quantized directly to signed 16-bit PCM without another normalization pass.
`pcm_909.rs` reads the bytes explicitly, so host endianness does not matter.

| Asset | Frames | SHA-256 |
|---|---:|---|
| `hat.pcm` | 24,784 | `1ba24e00c4cb1cee79468d65210e33d55d173535df8c62eefaa46906b46117de` |
| `closed-envelope-f32.table` | 13,920 | `770419dd57b18ef51818c3e9a61e0fca7456496dc613d7a1920eba5d8a814fe9` |
| `crash.pcm` | 23,257 | `e88bc7e4f3fc16b873f5261d49be29d26aac32ac1aeda8bffdcf1e6d1d665f9f` |
| `ride.pcm` | 21,737 | `80ef8bd2143ddbccf99daffbede532e91bdfdf723a2f7e48152413e6d56edbea` |

`sinc8-256-f32.table` is unchanged project-generated interpolation data: 256 fractional phases x 16
Hann-windowed sinc taps, each row normalized to unity DC gain and stored as little-endian `f32`.
