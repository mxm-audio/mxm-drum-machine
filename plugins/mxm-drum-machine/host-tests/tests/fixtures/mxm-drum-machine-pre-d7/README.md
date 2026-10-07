# mxm-drum-machine pre-D7 baseline

Project-generated compatibility fixture captured from the release bundle at the commit in `source-commit.txt`, before assignable outputs and chromatic MIDI channels existed.

- `mxm-drum-machine-pre-d7.clap` — fixed Windows x86_64 CLAP bundle rebuilt from that exact source commit with `--remap-path-prefix` so release evidence carries no local compiler paths; not loaded by routine tests. The state and render below came from the original equivalent build before D7 work began.
- `non-default-kit.clapstate` — CLAP state with Slot 1 Pitch 0.71, Decay 0.37 and Pan 0.79, plus Slot 4 Level 0.63, on the IDs of its day (`pitch_1`, `decay_1`, …). **Since 2026-10-07 the plugin no longer reads those IDs** — its controls are general, `c01`…`c20`, and nothing migrates old IDs in pre-alpha (the owner) — so the routine test rewrites each named control's ID to the general control it became (`behaviour.rs`'s `MAPPING`: Pitch → `c01`, Decay → `c02`, …) before loading it. The file itself is unchanged.
- `non-default-kit-main.wav` — project-generated stereo float WAV: note 36 at velocity 0.82, 48 blocks held and 16 blocks released, 512 frames per block at 48 kHz after a two-block settle. It is the pre-D7 render of the retained bundle, and proved D7's default routing bit-exact against it. Routine tests no longer load it.
- `non-default-kit-main-current.wav` — the same render from the current build, which the routine test compares bit-for-bit on Windows and within rounding elsewhere, because it holds Windows' bits (the owner, 2026-10-06). It was re-captured on 2026-09-19 because the slot's model, Deep bridge kick, was deliberately refitted to a comparison recording (plan revision 41). Re-capture it only for such a sound change, with `recapture_the_current_non_default_kit_render`, and record the reason and checksum here. On 2026-10-07 it held unchanged through the move to general controls and four route slots: `the_recorded_kit_on_the_general_controls_renders_its_recording_bit_exact` loads the state above through the ID mapping and must match it, which is what the retired pre-D7 test (`pre_d7_state_opens_with_routing_defaults_and_bit_exact_main_audio`) checked through the old IDs.

SHA-256:

```text
f7efc19dc888192a6a231f74d42dd85335de898107f5b7604878555c1fd14ae3  mxm-drum-machine-pre-d7.clap
ddd1ed74414c6f56db7264738e1c8cdf0c9751d32aac5f8ff5a04d1a9410abdb  non-default-kit-main.wav
d627f1bf68e74b3df5002a6738271c5b7b47aa369ee15bba16a0ba4ce48d6e86  non-default-kit-main-current.wav
0506f4b78d34b1701227c8fd317cf0821277baee27c12179c3cbf3b45f711629  non-default-kit.clapstate
```
