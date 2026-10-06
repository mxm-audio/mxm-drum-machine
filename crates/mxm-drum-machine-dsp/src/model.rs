//! Stable model identity and the currently implemented catalogue slice.
//!
//! The 0…255 domain is permanent while implementations grow in evidence-aligned batches. IDs are
//! assigned by `docs/briefs/mxm-drum-machine.md`; this module exposes only models that actually
//! make sound, so an editor never offers a silent placeholder.

/// Permanent parameter-domain maximum. Expanding the visible catalogue must not change this.
pub const MODEL_ID_MAX: u8 = u8::MAX;

/// A stored model ID, including values this build does not implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModelId(u8);

impl ModelId {
    pub const OFF: Self = Self(0);
    pub const DEEP_BRIDGE_KICK: Self = Self(1);
    pub const TWIN_MODE_SNARE: Self = Self(2);
    pub const LOW_FALLING_TOM: Self = Self(3);
    pub const LOW_FALLING_CONGA: Self = Self(4);
    pub const MID_FALLING_TOM: Self = Self(5);
    pub const MID_FALLING_CONGA: Self = Self(6);
    pub const HIGH_FALLING_TOM: Self = Self(7);
    pub const HIGH_FALLING_CONGA: Self = Self(8);
    pub const LAYERED_SHORT_RIM: Self = Self(9);
    pub const PURE_HIGH_CLAVE: Self = Self(10);
    pub const BRIGHT_SHORT_MARACA: Self = Self(11);
    pub const TRIPLE_PULSE_CLAP: Self = Self(12);
    pub const TWIN_SQUARE_COWBELL: Self = Self(13);
    pub const THREE_PATH_CYMBAL: Self = Self(14);
    pub const SIX_SQUARE_CLOSED_HAT: Self = Self(15);
    pub const SIX_SQUARE_OPEN_HAT: Self = Self(16);
    pub const RESET_PUNCH_KICK: Self = Self(17);
    pub const RESET_TWIN_SNARE: Self = Self(18);
    pub const LOW_RESET_TRIAD_TOM: Self = Self(19);
    pub const MID_RESET_TRIAD_TOM: Self = Self(20);
    pub const HIGH_RESET_TRIAD_TOM: Self = Self(21);
    pub const TRIPLE_RESONATOR_RIM: Self = Self(22);
    pub const FOUR_CELL_CLAP: Self = Self(23);
    pub const SIX_BIT_CLOSED_HAT: Self = Self(24);
    pub const SIX_BIT_OPEN_HAT: Self = Self(25);
    pub const SIX_BIT_CRASH: Self = Self(26);
    pub const SIX_BIT_RIDE: Self = Self(27);
    pub const ECONOMY_62_KICK: Self = Self(28);
    pub const ECONOMY_BODY_SNARE: Self = Self(29);
    pub const ECONOMY_SHORT_RIM: Self = Self(30);
    pub const INDUCTOR_NOISE_HAT: Self = Self(31);

    #[must_use]
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Unimplemented and future IDs are silent, never aliases of an existing circuit.
    #[must_use]
    pub const fn implemented(self) -> ImplementedModel {
        match self.0 {
            1 => ImplementedModel::DeepBridgeKick,
            2 => ImplementedModel::TwinModeSnare,
            3 => ImplementedModel::LowFallingTom,
            4 => ImplementedModel::LowFallingConga,
            5 => ImplementedModel::MidFallingTom,
            6 => ImplementedModel::MidFallingConga,
            7 => ImplementedModel::HighFallingTom,
            8 => ImplementedModel::HighFallingConga,
            9 => ImplementedModel::LayeredShortRim,
            10 => ImplementedModel::PureHighClave,
            11 => ImplementedModel::BrightShortMaraca,
            12 => ImplementedModel::TriplePulseClap,
            13 => ImplementedModel::TwinSquareCowbell,
            14 => ImplementedModel::ThreePathCymbal,
            15 => ImplementedModel::SixSquareClosedHat,
            16 => ImplementedModel::SixSquareOpenHat,
            17 => ImplementedModel::ResetPunchKick,
            18 => ImplementedModel::ResetTwinSnare,
            19 => ImplementedModel::LowResetTriadTom,
            20 => ImplementedModel::MidResetTriadTom,
            21 => ImplementedModel::HighResetTriadTom,
            22 => ImplementedModel::TripleResonatorRim,
            23 => ImplementedModel::FourCellClap,
            24 => ImplementedModel::SixBitClosedHat,
            25 => ImplementedModel::SixBitOpenHat,
            26 => ImplementedModel::SixBitCrash,
            27 => ImplementedModel::SixBitRide,
            28 => ImplementedModel::Economy62Kick,
            29 => ImplementedModel::EconomyBodySnare,
            30 => ImplementedModel::EconomyShortRim,
            31 => ImplementedModel::InductorNoiseHat,
            32..=94 => ImplementedModel::Legacy(self.0),
            _ => ImplementedModel::Off,
        }
    }

    /// Fixed-domain conversion used by the plugin's stepped parameter.
    #[must_use]
    pub fn normalised(self) -> f32 {
        f32::from(self.0) / f32::from(MODEL_ID_MAX)
    }

    /// Inverse of [`ModelId::normalised`], with hostile values made harmless.
    #[must_use]
    pub fn from_normalised(value: f32) -> Self {
        let finite = if value.is_finite() { value } else { 0.0 };
        Self((finite.clamp(0.0, 1.0) * f32::from(MODEL_ID_MAX)).round() as u8)
    }
}

/// Models with renderers in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementedModel {
    Off,
    DeepBridgeKick,
    TwinModeSnare,
    LowFallingTom,
    LowFallingConga,
    MidFallingTom,
    MidFallingConga,
    HighFallingTom,
    HighFallingConga,
    LayeredShortRim,
    PureHighClave,
    BrightShortMaraca,
    TriplePulseClap,
    TwinSquareCowbell,
    ThreePathCymbal,
    SixSquareClosedHat,
    SixSquareOpenHat,
    ResetPunchKick,
    ResetTwinSnare,
    LowResetTriadTom,
    MidResetTriadTom,
    HighResetTriadTom,
    TripleResonatorRim,
    FourCellClap,
    SixBitClosedHat,
    SixBitOpenHat,
    SixBitCrash,
    SixBitRide,
    Economy62Kick,
    EconomyBodySnare,
    EconomyShortRim,
    InductorNoiseHat,
    Legacy(u8),
}

/// One assigned model offered by the selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: ModelId,
    pub label: &'static str,
    pub group: &'static str,
}

/// Common creative axes that have an honest interpretation for one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub pitch: bool,
    pub pitch_envelope: bool,
    pub pitch_decay: bool,
    pub decay: bool,
    pub attack: bool,
    pub tone: bool,
    pub body: bool,
    pub noise: bool,
    pub noise_decay: bool,
    pub character: bool,
    pub dynamics: bool,
}

impl ModelId {
    #[must_use]
    pub const fn capabilities(self) -> Capabilities {
        const P: u8 = 1 << 0;
        const D: u8 = 1 << 1;
        const A: u8 = 1 << 2;
        const T: u8 = 1 << 3;
        const B: u8 = 1 << 4;
        const N: u8 = 1 << 5;
        const C: u8 = 1 << 6;
        const Y: u8 = 1 << 7;
        let bits = match self.0 {
            1 | 9 | 10 => P | D | A | T | B | C | Y,
            2 | 3 => P | D | A | T | B | N | Y,
            4..=8 => P | D | A | T | B | Y,
            11 => D | A | T | N | Y,
            12 => D | A | T | N | C | Y,
            13..=16 => P | D | A | T | C | Y,
            17..=18 => P | D | A | T | B | N | Y,
            19..=21 => P | D | A | T | B | N | C | Y,
            22 => P | D | A | T | B | C | Y,
            23 => D | A | T | N | C | Y,
            24..=27 => P | D | A | T | C | Y,
            28 => P | D | A | T | B | Y,
            29 => P | D | A | T | B | N | Y,
            30 => P | D | A | T | B | Y,
            31 => D | A | T | N | C | Y,
            32 | 40 | 45 | 54 | 73 | 78 | 89 => P | D | A | T | B | C | Y,
            33 | 34 | 35 | 36 | 48 | 55 | 61 | 81 | 91 => P | D | A | T | B | N | Y,
            37 | 38 | 39 | 47 | 49 | 50 | 60 | 62 | 66 | 67 | 68 | 69 | 74 | 75 | 76 | 77 | 79
            | 80 | 85 | 86 | 87 | 88 | 90 => P | D | A | T | B | Y,
            41 | 42 | 43 | 46 | 51 | 52 | 53 | 70 | 72 => P | D | A | T | C | Y,
            44 | 59 | 63 | 64 | 71 | 82 | 84 | 92 | 94 => D | A | T | N | C | Y,
            56..=58 => P | D | A | T | N | C | Y,
            65 | 83 | 93 => D | A | T | N | Y,
            _ => 0,
        };
        let pitch_envelope = matches!(
            self.0,
            1 | 3..=8
                | 17..=21
                | 28
                | 32
                | 34..=39
                | 47
                | 49..=50
                | 54
                | 60
                | 67..=69
                | 74..=77
                | 85..=88
        );
        let noise_decay = matches!(
            self.0,
            2 | 3
                | 12
                | 17..=21
                | 23
                | 29
                | 33..=36
                | 44
                | 48
                | 55
                | 59
                | 61
                | 81
                | 91
                | 94
        );
        Capabilities::from_bits(bits, pitch_envelope, noise_decay)
    }
}

impl ModelId {
    /// The **rest pitch** of a pitched model at zero deviation, in Hz: where its pitch settles once
    /// any sweep is over. Every model with a tonal body has one — kicks, snares (their body), toms,
    /// congas, bongos, cowbells, claves, rims and the bell. `None` for hats, cymbals, claps,
    /// maracas, the tambourine, guiro and brush, whose sound is metal clusters or noise with no note
    /// to play in tune, even where Tune moves their colour (owner, 2026-09-18).
    ///
    /// **Measured, not assumed**, by `tests/musical_pitch.rs`, the owner's method: each model is
    /// rendered in an analysis patch that isolates its tone — noise and its decay down, the pitch
    /// sweep off, the body's decay up — and read over the late, stable part of the ring, whose two
    /// halves must agree. The value is the lowest partial there within 12 dB of the strongest.
    /// Averaging over a whole hit reads a kick's sweep instead: up to 84 cents off. The test also
    /// proves those controls do not move the rest pitch, and that every model plays within 5 cents
    /// an octave either way. Changing a pitched model's tone or pitch law means re-running
    /// `print_the_measured_reference_pitches` and updating this table.
    #[must_use]
    pub const fn reference_pitch_hz(self) -> Option<f32> {
        match self.0 {
            1 => Some(48.80),    // Deep bridge kick
            2 => Some(168.30),   // Twin-mode snare
            3 => Some(85.50),    // Low falling tom
            4 => Some(186.00),   // Low falling conga
            5 => Some(132.70),   // Mid falling tom
            6 => Some(275.00),   // Mid falling conga
            7 => Some(181.10),   // High falling tom
            8 => Some(391.20),   // High falling conga
            9 => Some(472.00),   // Layered short rim
            10 => Some(2497.00), // Pure high clave
            13 => Some(824.65),  // Twin-square cowbell
            17 => Some(54.00),   // Reset punch kick
            18 => Some(178.00),  // Reset twin snare
            19 => Some(88.50),   // Low reset triad tom
            20 => Some(120.00),  // Mid reset triad tom
            21 => Some(138.70),  // High reset triad tom
            22 => Some(220.00),  // Triple-resonator rim
            28 => Some(62.01),   // Economy 62 kick
            29 => Some(350.00),  // Economy body snare
            30 => Some(1350.00), // Economy short rim
            32 => Some(61.00),   // Dual-low kick
            33 => Some(222.40),  // Dual-bridge snare
            34 => Some(103.00),  // Diode low tom
            35 => Some(140.00),  // Diode mid tom
            36 => Some(197.00),  // Diode high tom
            37 => Some(299.40),  // Diode low conga
            38 => Some(436.30),  // Diode mid conga
            39 => Some(633.00),  // Diode high conga
            40 => Some(1300.00), // Thirty-millisecond rim
            45 => Some(2246.00), // Phase-shift clave
            46 => Some(838.15),  // Split-square cowbell
            47 => Some(61.40),   // Compact dual kick
            48 => Some(208.00),  // Compact body snare
            49 => Some(131.00),  // Compact low tom
            50 => Some(207.00),  // Compact high tom
            54 => Some(50.40),   // Damped whack kick
            55 => Some(243.00),  // LFSR snap snare
            60 => Some(59.40),   // Classic 62 kick
            61 => Some(302.00),  // Classic 340 snare
            62 => Some(1405.00), // Five-millisecond rim
            66 => Some(2630.00), // High 2630 clave
            67 => Some(591.00),  // High 600 bongo
            68 => Some(387.00),  // Low 400 bongo
            69 => Some(196.50),  // Low 208 conga
            70 => Some(545.00),  // Close-interval cowbell
            73 => Some(4080.00), // Triple high bell
            74 => Some(62.70),   // Discrete 62 kick
            75 => Some(208.00),  // Discrete 208 conga
            76 => Some(400.00),  // Discrete low bongo
            77 => Some(600.00),  // Discrete high bongo
            78 => Some(830.00),  // Resonant 830 cowbell
            79 => Some(1471.00), // Discrete short rim
            80 => Some(2341.00), // Resonant 2350 clave
            81 => Some(273.00),  // Bongo-body snare
            85 => Some(51.00),   // Early transistor kick
            86 => Some(190.00),  // Early low conga
            87 => Some(290.00),  // Early high conga
            88 => Some(520.00),  // Early high bongo
            89 => Some(540.00),  // Early cowbell
            90 => Some(353.00),  // Early clave
            91 => Some(274.00),  // Early snare
            _ => None,
        }
    }

    /// The MIDI key a chromatic slot plays this model at, unmoved (owner, 2026-09-18). A pitched
    /// model sits at its own concert pitch, so a keyboard plays it in tune like an instrument
    /// (A4 = note 69 = 440 Hz, twelve-tone equal temperament); any other model is reached at note 60.
    #[must_use]
    pub fn chromatic_reference_key(self) -> f32 {
        self.reference_pitch_hz()
            .map_or(60.0, |hz| 69.0 + 12.0 * (hz / 440.0).log2())
    }
}

impl Capabilities {
    const fn from_bits(bits: u8, pitch_envelope: bool, noise_decay: bool) -> Self {
        Self {
            pitch: bits & (1 << 0) != 0,
            pitch_envelope,
            pitch_decay: pitch_envelope,
            decay: bits & (1 << 1) != 0,
            attack: bits & (1 << 2) != 0,
            tone: bits & (1 << 3) != 0,
            body: bits & (1 << 4) != 0,
            noise: bits & (1 << 5) != 0,
            noise_decay,
            character: bits & (1 << 6) != 0,
            dynamics: bits & (1 << 7) != 0,
        }
    }
}

/// Models offered for new assignments, in permanent ID order. New entries append. ID 0 remains
/// a permanent compatibility value and still renders silence, but intentional silence now belongs
/// to a slot's Mute parameter rather than this catalogue.
pub const AVAILABLE_MODELS: [ModelSpec; 94] = [
    ModelSpec {
        id: ModelId::DEEP_BRIDGE_KICK,
        label: "Deep bridge kick",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::TWIN_MODE_SNARE,
        label: "Twin-mode snare",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::LOW_FALLING_TOM,
        label: "Low falling tom",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::LOW_FALLING_CONGA,
        label: "Low falling conga",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::MID_FALLING_TOM,
        label: "Mid falling tom",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::MID_FALLING_CONGA,
        label: "Mid falling conga",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::HIGH_FALLING_TOM,
        label: "High falling tom",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::HIGH_FALLING_CONGA,
        label: "High falling conga",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::LAYERED_SHORT_RIM,
        label: "Layered short rim",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::PURE_HIGH_CLAVE,
        label: "Pure high clave",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::BRIGHT_SHORT_MARACA,
        label: "Bright short maraca",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::TRIPLE_PULSE_CLAP,
        label: "Triple pulse clap",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::TWIN_SQUARE_COWBELL,
        label: "Twin-square cowbell",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::THREE_PATH_CYMBAL,
        label: "Three-path cymbal",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::SIX_SQUARE_CLOSED_HAT,
        label: "Six-square closed hat",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::SIX_SQUARE_OPEN_HAT,
        label: "Six-square open hat",
        group: "Bridge 808",
    },
    ModelSpec {
        id: ModelId::RESET_PUNCH_KICK,
        label: "Reset punch kick",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::RESET_TWIN_SNARE,
        label: "Reset twin snare",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::LOW_RESET_TRIAD_TOM,
        label: "Low reset triad tom",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::MID_RESET_TRIAD_TOM,
        label: "Mid reset triad tom",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::HIGH_RESET_TRIAD_TOM,
        label: "High reset triad tom",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::TRIPLE_RESONATOR_RIM,
        label: "Triple-resonator rim",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::FOUR_CELL_CLAP,
        label: "Four-cell clap",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::SIX_BIT_CLOSED_HAT,
        label: "Six-bit closed hat",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::SIX_BIT_OPEN_HAT,
        label: "Six-bit open hat",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::SIX_BIT_CRASH,
        label: "Six-bit crash",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::SIX_BIT_RIDE,
        label: "Six-bit ride",
        group: "Reset 909",
    },
    ModelSpec {
        id: ModelId::ECONOMY_62_KICK,
        label: "Economy 62 kick",
        group: "Economy 55",
    },
    ModelSpec {
        id: ModelId::ECONOMY_BODY_SNARE,
        label: "Economy body snare",
        group: "Economy 55",
    },
    ModelSpec {
        id: ModelId::ECONOMY_SHORT_RIM,
        label: "Economy short rim",
        group: "Economy 55",
    },
    ModelSpec {
        id: ModelId::INDUCTOR_NOISE_HAT,
        label: "Inductor noise hat",
        group: "Economy 55",
    },
    ModelSpec {
        id: ModelId::new(32),
        label: "Dual-low kick",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(33),
        label: "Dual-bridge snare",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(34),
        label: "Diode low tom",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(35),
        label: "Diode mid tom",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(36),
        label: "Diode high tom",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(37),
        label: "Diode low conga",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(38),
        label: "Diode mid conga",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(39),
        label: "Diode high conga",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(40),
        label: "Thirty-millisecond rim",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(41),
        label: "Six-square cymbal",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(42),
        label: "Six-square short hat",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(43),
        label: "Six-square long hat",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(44),
        label: "Saw-noise clap",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(45),
        label: "Phase-shift clave",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(46),
        label: "Split-square cowbell",
        group: "Expanded 8000",
    },
    ModelSpec {
        id: ModelId::new(47),
        label: "Compact dual kick",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(48),
        label: "Compact body snare",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(49),
        label: "Compact low tom",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(50),
        label: "Compact high tom",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(51),
        label: "Two-band cymbal",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(52),
        label: "Resonant closed hat",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(53),
        label: "Tempo-coupled open hat",
        group: "Compact 606",
    },
    ModelSpec {
        id: ModelId::new(54),
        label: "Damped whack kick",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(55),
        label: "LFSR snap snare",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(56),
        label: "Mixed-source cymbal",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(57),
        label: "Mixed-source closed hat",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(58),
        label: "Mixed-source open hat",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(59),
        label: "Timed-burst clap",
        group: "Snap 110",
    },
    ModelSpec {
        id: ModelId::new(60),
        label: "Classic 62 kick",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(61),
        label: "Classic 340 snare",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(62),
        label: "Five-millisecond rim",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(63),
        label: "Sixty-millisecond noise hat",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(64),
        label: "Long noise cymbal",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(65),
        label: "Twenty-millisecond maraca",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(66),
        label: "High 2630 clave",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(67),
        label: "High 600 bongo",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(68),
        label: "Low 400 bongo",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(69),
        label: "Low 208 conga",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(70),
        label: "Close-interval cowbell",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(71),
        label: "Tambourine wash",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(72),
        label: "Two-rate guiro",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(73),
        label: "Triple high bell",
        group: "Classic 78",
    },
    ModelSpec {
        id: ModelId::new(74),
        label: "Discrete 62 kick",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(75),
        label: "Discrete 208 conga",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(76),
        label: "Discrete low bongo",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(77),
        label: "Discrete high bongo",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(78),
        label: "Resonant 830 cowbell",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(79),
        label: "Discrete short rim",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(80),
        label: "Resonant 2350 clave",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(81),
        label: "Bongo-body snare",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(82),
        label: "Forty-millisecond noise hat",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(83),
        label: "Forty-millisecond maraca",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(84),
        label: "Four-hundred-millisecond cymbal",
        group: "Discrete 66",
    },
    ModelSpec {
        id: ModelId::new(85),
        label: "Early transistor kick",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(86),
        label: "Early low conga",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(87),
        label: "Early high conga",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(88),
        label: "Early high bongo",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(89),
        label: "Early cowbell",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(90),
        label: "Early clave",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(91),
        label: "Early snare",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(92),
        label: "Early cymbal",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(93),
        label: "Early maraca",
        group: "Early 2L",
    },
    ModelSpec {
        id: ModelId::new(94),
        label: "Early wire brush",
        group: "Early 2L",
    },
];

#[must_use]
pub fn available(id: ModelId) -> Option<&'static ModelSpec> {
    AVAILABLE_MODELS.iter().find(|spec| spec.id == id)
}

#[must_use]
pub fn label(id: ModelId) -> &'static str {
    available(id).map_or("Unavailable", |spec| spec.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_ids_are_in_stable_order_and_labels_are_unique() {
        for (index, spec) in AVAILABLE_MODELS.iter().enumerate() {
            if let Some(previous) = index.checked_sub(1).map(|i| AVAILABLE_MODELS[i].id.raw()) {
                assert!(
                    previous < spec.id.raw(),
                    "available model IDs must stay ordered"
                );
            }
            assert!(!spec.label.is_empty());
            assert!(!spec.group.is_empty());
            assert_eq!(
                AVAILABLE_MODELS
                    .iter()
                    .filter(|other| other.label == spec.label)
                    .count(),
                1,
                "duplicate public model label {:?}",
                spec.label
            );
        }
    }

    #[test]
    fn selector_groups_follow_the_factory_family_order() {
        const EXPECTED: [&str; 9] = [
            "Bridge 808",
            "Reset 909",
            "Economy 55",
            "Expanded 8000",
            "Compact 606",
            "Snap 110",
            "Classic 78",
            "Discrete 66",
            "Early 2L",
        ];
        let mut actual = Vec::new();
        for spec in &AVAILABLE_MODELS {
            if actual.last().copied() != Some(spec.group) {
                actual.push(spec.group);
            }
        }
        assert_eq!(actual, EXPECTED);
    }

    #[test]
    fn legacy_off_is_silent_but_not_offered_for_new_assignments() {
        assert!(available(ModelId::OFF).is_none());
        assert_eq!(ModelId::OFF.implemented(), ImplementedModel::Off);
    }

    #[test]
    fn product_labels_do_not_use_reference_maker_or_model_names() {
        const FORBIDDEN: [&str; 4] = ["roland", "tr-808", "808", "rhythm composer"];
        for spec in AVAILABLE_MODELS {
            let label = spec.label.to_ascii_lowercase();
            for word in FORBIDDEN {
                assert!(!label.contains(word), "{:?} contains {word:?}", spec.label);
            }
        }
    }

    #[test]
    fn every_domain_value_round_trips_without_catalogue_size_in_the_arithmetic() {
        for raw in 0..=u8::MAX {
            let id = ModelId::new(raw);
            assert_eq!(ModelId::from_normalised(id.normalised()), id);
        }
    }

    #[test]
    fn capability_rows_match_the_permanent_axis_ledger() {
        let maraca = ModelId::BRIGHT_SHORT_MARACA.capabilities();
        assert!(!maraca.pitch && !maraca.body && !maraca.character);
        assert!(maraca.decay && maraca.attack && maraca.tone && maraca.noise && maraca.dynamics);
        let clap = ModelId::TRIPLE_PULSE_CLAP.capabilities();
        assert!(!clap.pitch && !clap.body);
        assert!(clap.character && clap.noise && clap.noise_decay);
        let reset_kick = ModelId::RESET_PUNCH_KICK.capabilities();
        assert!(reset_kick.pitch_envelope && reset_kick.pitch_decay && reset_kick.noise_decay);
        let pcm_hat = ModelId::SIX_BIT_OPEN_HAT.capabilities();
        assert!(!pcm_hat.pitch_envelope && !pcm_hat.pitch_decay && !pcm_hat.noise_decay);
        assert_eq!(
            ModelId::new(200).capabilities(),
            ModelId::OFF.capabilities()
        );
    }

    #[test]
    fn future_and_hostile_ids_are_silent_not_aliases() {
        for raw in [95, 96, 254, 255] {
            assert_eq!(ModelId::new(raw).implemented(), ImplementedModel::Off);
            assert_eq!(available(ModelId::new(raw)), None);
        }
        assert_eq!(ModelId::from_normalised(f32::NAN), ModelId::OFF);
    }
}
