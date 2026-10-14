//! Each slot's name (the owner, 2026-10-14: "make the software make a 'screen name' like bd1, and
//! then give the user the ability to rename it to anything like bd1 -> subkick").
//!
//! - **A slot is called what it is**: its model's kind and how many slots before it play that
//!   kind, plus one (`mxm_drum_machine_dsp::model::made_up_names`): `bd1`, `sn1`, `ch1`.
//! - **A name typed over it is the song's**: a persistent field, saved with the plugin's state,
//!   which a kit never carries. Loading a kit, or Init, empties every typed name, since the drums
//!   changed (`Instrument::patch_replaced`); changing one slot's model keeps its name. An empty name
//!   follows the model again, as an empty lane name in newDAWn follows the instrument.
//! - **The host is told** through CLAP's note-name extension, the nice-plug fork's capability: each
//!   Kit slot's key carries its name, so a host's drum lanes read as the slots do.
//!
//! Every write of the names raises [`MxmDrumMachineParams::names_written`]; the editor then sends
//! the fork's dirty-only GUI transaction ([`announce`]), which tells the host the song changed and
//! makes the wrapper read the note names again.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::model::{available, made_up_names};
use mxm_part_routing::{PartAssignment, claimed_channels};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::{ClapNoteNames, NoteName};

use crate::params::{MxmDrumMachineParams, at, slot_ids};

/// The persistent field's key in the plugin's state.
pub const FIELD: &str = "slot-names";

/// The most of a typed name that is kept, in characters (Unicode scalar values, not bytes).
pub const NAME_CHARS: usize = 32;

/// Every slot's name empty: each follows its model.
#[must_use]
pub fn empty() -> Vec<String> {
    vec![String::new(); SLOT_COUNT]
}

/// A typed name as it is kept: without control characters, trimmed, and at most [`NAME_CHARS`]
/// characters.
#[must_use]
pub fn canonical_name(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    let cut: String = clean.trim().chars().take(NAME_CHARS).collect();
    cut.trim_end().to_owned()
}

/// The names as they are kept, from whatever a state held under [`FIELD`]: a list cut or padded to
/// `SLOT_COUNT`, each name [canonical](canonical_name). Nothing there, or anything but a list,
/// is every name empty, and an entry that isn't text is an empty name: the names are a convenience,
/// never worth refusing a song over.
#[must_use]
pub fn canonical(stored: Option<&str>) -> Vec<String> {
    let listed: Vec<String> = stored
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| match value {
            serde_json::Value::Array(items) => Some(items),
            _ => None,
        })
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(canonical_name).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    (0..SLOT_COUNT)
        .map(|slot| listed.get(slot).cloned().unwrap_or_default())
        .collect()
}

/// `Plugin::filter_state`'s part, run on every state before it is restored: the names' field
/// rewritten to its canonical text. An older state without it gets it, every name empty, so
/// opening it empties names typed since rather than keeping them. The field then installs exactly
/// what arrived, which the fork's check on persistent fields requires.
pub fn filter(fields: &mut BTreeMap<String, String>) {
    let names = canonical(fields.get(FIELD).map(String::as_str));
    let text = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_owned());
    fields.insert(FIELD.to_owned(), text);
}

fn typed(params: &MxmDrumMachineParams) -> Vec<String> {
    params
        .slot_names
        .read()
        .map(|names| names.clone())
        .unwrap_or_default()
}

/// What each slot is called: the name typed over it, or else the one it is given; none for a slot
/// with nothing typed whose model is Off or unavailable.
#[must_use]
pub fn shown(params: &MxmDrumMachineParams) -> [Option<String>; SLOT_COUNT] {
    let models = std::array::from_fn(|slot| params.slots[slot].model_id());
    let made_up = made_up_names(&models);
    let typed = typed(params);
    std::array::from_fn(|slot| match typed.get(slot) {
        Some(name) if !name.is_empty() => Some(name.clone()),
        _ => made_up[slot].clone(),
    })
}

/// The name a slot would have with nothing typed over it.
#[must_use]
pub fn made_up(params: &MxmDrumMachineParams, slot: usize) -> Option<String> {
    let models = std::array::from_fn(|slot| params.slots[slot].model_id());
    made_up_names(&models)[slot].clone()
}

/// Types a slot's name. It is kept [canonical](canonical_name), and empty when it is the name the
/// slot is given anyway, so it goes on following its model.
pub fn set(params: &MxmDrumMachineParams, slot: usize, text: &str) {
    let mut name = canonical_name(text);
    if made_up(params, slot).as_deref() == Some(name.as_str()) {
        name.clear();
    }
    let Ok(mut names) = params.slot_names.write() else {
        return;
    };
    if names.len() != SLOT_COUNT {
        *names = empty();
    }
    if names[slot] != name {
        names[slot] = name;
        params.names_written.store(true, Ordering::Release);
    }
}

/// Empties every typed name: a kit load and Init (`Instrument::patch_replaced`).
pub fn clear(params: &MxmDrumMachineParams) {
    let Ok(mut names) = params.slot_names.write() else {
        return;
    };
    if names.iter().any(|name| !name.is_empty()) || names.len() != SLOT_COUNT {
        *names = empty();
        params.names_written.store(true, Ordering::Release);
    }
}

/// The editor's half of every write, once a frame: when the names changed, nice-plug's dirty-only
/// GUI transaction (no parameters, the fields as they are now), which tells the host the song
/// changed and makes the wrapper read the note names again (`PATCHES.md`, *GUI-authored state
/// dirty* and *Note names*).
pub fn announce(params: &MxmDrumMachineParams, context: &GuiContext) {
    if params.names_written.swap(false, Ordering::AcqRel) {
        let mut state = context.get_state();
        state.params.clear();
        context.set_state(state);
    }
}

/// The names the host is told: CLAP note names, through the fork's capability.
pub struct HostNames(pub Arc<MxmDrumMachineParams>);

impl ClapNoteNames for HostNames {
    /// Each Kit slot whose model plays, on its key and the plugin's one note port, on every channel a
    /// Kit key sounds on: any (`-1`) while no slot claims a channel, otherwise each unclaimed one.
    /// A chromatic slot plays every key on its channel, so it names none.
    fn names(&self) -> Vec<NoteName> {
        let params = &self.0;
        let assignments: [PartAssignment; SLOT_COUNT] = std::array::from_fn(|slot| {
            PartAssignment::from_parameter(params.slots[slot].midi_channel.value())
        });
        let claimed = claimed_channels(&assignments);
        let channels: Vec<i16> = if claimed == 0 {
            vec![-1]
        } else {
            (0..16_i16)
                .filter(|channel| claimed & (1 << channel) == 0)
                .collect()
        };
        let shown = shown(params);
        let mut names = Vec::new();
        for (slot, name) in shown.into_iter().enumerate() {
            let plays = available(params.slots[slot].model_id()).is_some();
            let Some(name) =
                name.filter(|_| plays && assignments[slot] == PartAssignment::FixedNote)
            else {
                continue;
            };
            let key = i16::from(crate::FIRST_KEY) + slot as i16;
            names.extend(channels.iter().map(|&channel| NoteName {
                name: name.clone(),
                port: 0,
                key,
                channel,
            }));
        }
        names
    }

    /// A slot's model gives its made-up name and whether it plays; its MIDI channel whether it is a
    /// Kit slot, and which channels a Kit key sounds on.
    fn depends_on(&self) -> Vec<&'static str> {
        (0..SLOT_COUNT)
            .flat_map(|slot| {
                let ids = slot_ids(slot);
                [ids[at::MODEL], ids[at::MIDI_CHANNEL]]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_drum_machine_dsp::model::AVAILABLE_MODELS;
    use nice_plug::params::InternalParamMut;
    use nice_plug::prelude::ParamSetter;

    /// The Init kit's slots, each called what it is.
    const INIT: [&str; SLOT_COUNT] = [
        "bd1", "bd2", "sn1", "sn2", "tm1", "tm2", "tm3", "rs1", "cp1", "cp2", "cb1", "ch1", "oh1",
        "cy1", "cr1", "rd1",
    ];

    fn shown_names(params: &MxmDrumMachineParams) -> Vec<String> {
        shown(params)
            .into_iter()
            .map(|name| name.unwrap_or_else(|| "-".to_owned()))
            .collect()
    }

    fn set_channel(params: &MxmDrumMachineParams, slot: usize, value: i32) {
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            params.slots[slot]
                .midi_channel
                ._internal_set_plain_value(value)
        };
    }

    fn set_model(params: &MxmDrumMachineParams, slot: usize, raw: u8) {
        // SAFETY: as above.
        unsafe {
            params.slots[slot]
                .model
                ._internal_set_plain_value(i32::from(raw))
        };
    }

    fn written(params: &MxmDrumMachineParams) -> bool {
        params.names_written.swap(false, Ordering::AcqRel)
    }

    #[test]
    fn the_init_kit_is_called_what_its_drums_are() {
        let params = MxmDrumMachineParams::default();
        assert_eq!(shown_names(&params), INIT);
    }

    #[test]
    fn a_typed_name_wins_and_an_empty_or_the_given_one_follows_the_model() {
        let params = MxmDrumMachineParams::default();
        set(&params, 0, "  subkick ");
        assert!(written(&params));
        assert_eq!(shown(&params)[0].as_deref(), Some("subkick"));
        assert_eq!(params.slot_names.read().unwrap()[0], "subkick");

        // Changing the slot's model keeps its name.
        set_model(&params, 0, AVAILABLE_MODELS[1].id.raw());
        assert_eq!(shown(&params)[0].as_deref(), Some("subkick"));

        set(&params, 0, "");
        assert!(written(&params));
        assert_eq!(
            shown(&params)[0].as_deref(),
            Some("sn1"),
            "the model's again"
        );

        // Typing the name it is given keeps nothing, so it goes on following the model.
        set(&params, 1, "bd1");
        assert_eq!(params.slot_names.read().unwrap()[1], "");
        set(&params, 1, "bd1");
        assert!(!written(&params), "nothing changed, nothing to tell");
    }

    #[test]
    fn a_kit_load_and_init_give_every_slot_its_name_back() {
        use mxm_preset::{Category, Instrument, Preset};
        let params = MxmDrumMachineParams::default();
        let host = mxm_plugin_test::NoHost;
        let setter = ParamSetter::new(&host);
        let kit = Preset::capture("Mine", Category::Uncategorised, &params);
        assert!(kit.state.is_none(), "a kit carries no names");

        set(&params, 0, "subkick");
        written(&params);
        assert!(mxm_preset::ui::apply_preset_checked(&params, &setter, &kit).0);
        assert_eq!(shown(&params)[0].as_deref(), Some("bd1"));
        assert!(written(&params), "the editor tells the host");

        set(&params, 2, "crack");
        written(&params);
        mxm_preset::ui::init_patch(&params, &setter);
        assert_eq!(shown(&params)[2].as_deref(), Some("sn1"));
        assert!(written(&params));

        // With nothing typed, there is nothing to tell.
        params.patch_replaced();
        assert!(!written(&params));
    }

    #[test]
    fn names_are_kept_canonical() {
        assert_eq!(canonical_name("  sub\u{7}kick\n "), "subkick");
        let long = "ø".repeat(40);
        assert_eq!(
            canonical_name(&long).chars().count(),
            NAME_CHARS,
            "cut in characters, not bytes"
        );
        assert_eq!(canonical(None), empty(), "an older state");
        assert_eq!(canonical(Some("{\"a\": 1}")), empty(), "not a list");
        assert_eq!(canonical(Some("not json")), empty());
        let short = canonical(Some("[\"kick\", 4, \" snare \"]"));
        assert_eq!(short.len(), SLOT_COUNT);
        assert_eq!(&short[..3], ["kick", "", "snare"]);
        let many: Vec<String> = (0..40).map(|n| format!("x{n}")).collect();
        let long_list = canonical(Some(&serde_json::to_string(&many).unwrap()));
        assert_eq!(long_list.len(), SLOT_COUNT);
        assert_eq!(long_list[15], "x15");
    }

    #[test]
    fn the_host_hears_each_kit_slot_on_its_key() {
        let params = MxmDrumMachineParams::default();
        let names = HostNames(Arc::new(params)).names();
        let heard: Vec<(String, i16, i16, i16)> = names
            .into_iter()
            .map(|name| (name.name, name.port, name.key, name.channel))
            .collect();
        let expected: Vec<(String, i16, i16, i16)> = INIT
            .iter()
            .enumerate()
            .map(|(slot, name)| ((*name).to_owned(), 0, 36 + slot as i16, -1))
            .collect();
        assert_eq!(heard, expected);
    }

    #[test]
    fn a_claimed_channel_takes_its_slot_out_and_names_the_rest_on_the_others() {
        let params = Arc::new(MxmDrumMachineParams::default());
        // The last slot plays every note on Ch 2 (CLAP channel 1).
        set_channel(&params, 15, 2);
        set(&params, 0, "subkick");
        let names = HostNames(params.clone()).names();
        assert!(
            names.iter().all(|name| name.key != 51),
            "chromatic: no key of its own"
        );
        assert!(
            names.iter().all(|name| name.channel != 1),
            "nor a Kit key on its channel"
        );
        let kick: Vec<i16> = names
            .iter()
            .filter(|name| name.key == 36)
            .map(|name| name.channel)
            .collect();
        assert_eq!(kick, [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        assert!(
            names
                .iter()
                .filter(|name| name.key == 36)
                .all(|name| name.name == "subkick")
        );
    }

    #[test]
    fn an_off_slot_names_nothing_to_the_host() {
        let params = Arc::new(MxmDrumMachineParams::default());
        set_model(&params, 3, 0);
        set(&params, 3, "ghost");
        let names = HostNames(params.clone()).names();
        assert!(names.iter().all(|name| name.key != 39), "Off plays nothing");
        assert_eq!(
            shown(&params)[3].as_deref(),
            Some("ghost"),
            "but the box keeps it"
        );
    }

    #[test]
    fn the_names_follow_each_slots_model_and_channel() {
        let ids = HostNames(Arc::new(MxmDrumMachineParams::default())).depends_on();
        assert_eq!(ids.len(), 2 * SLOT_COUNT);
        assert!(
            ids.contains(&"model_1") && ids.contains(&"midi_channel_16"),
            "{ids:?}"
        );
    }

    /// The real wrapper, as a host drives it: a state's names restored through the fork's whole
    /// restore (`filter_state` first, then its check on persistent fields), onto names already
    /// typed, and served through CLAP's note-name extension.
    mod through_the_wrapper {
        use std::ffi::{CStr, c_char, c_void};
        use std::sync::Arc;

        use clap_sys::ext::note_name::{CLAP_EXT_NOTE_NAME, clap_note_name, clap_plugin_note_name};
        use nice_plug::wrapper::clap::{CLAP_VERSION, Wrapper, clap_host};

        use crate::MxmDrumMachine;

        unsafe extern "C" fn no_extension(
            _host: *const clap_host,
            _id: *const c_char,
        ) -> *const c_void {
            std::ptr::null()
        }

        unsafe extern "C" fn nothing(_host: *const clap_host) {}

        fn host() -> Box<clap_host> {
            Box::new(clap_host {
                clap_version: CLAP_VERSION,
                host_data: std::ptr::null_mut(),
                name: c"slot-names-test".as_ptr(),
                vendor: c"mxm".as_ptr(),
                url: c"".as_ptr(),
                version: c"1".as_ptr(),
                get_extension: Some(no_extension),
                request_restart: Some(nothing),
                request_process: Some(nothing),
                request_callback: Some(nothing),
            })
        }

        fn stored(wrapper: &Wrapper<MxmDrumMachine>) -> String {
            wrapper.get_state_object().fields[super::FIELD].clone()
        }

        /// Restores a state whose names are `text`, or have no entry at all.
        fn restore(wrapper: &Wrapper<MxmDrumMachine>, text: Option<&str>) {
            let mut state = wrapper.get_state_object();
            match text {
                Some(text) => state
                    .fields
                    .insert(super::FIELD.to_owned(), text.to_owned()),
                None => state.fields.remove(super::FIELD),
            };
            wrapper.set_state_object_from_gui(state);
        }

        fn names(wrapper: &Wrapper<MxmDrumMachine>) -> Vec<(String, i16)> {
            let plugin = wrapper.clap_plugin.as_ptr();
            let get_extension = unsafe { (*plugin).get_extension }.unwrap();
            let extension = unsafe { get_extension(plugin, CLAP_EXT_NOTE_NAME.as_ptr()) };
            assert!(!extension.is_null(), "the drum machine names its notes");
            let extension = unsafe { &*extension.cast::<clap_plugin_note_name>() };
            let count = unsafe { extension.count.unwrap()(plugin) };
            (0..count)
                .map(|index| {
                    let mut name: clap_note_name = unsafe { std::mem::zeroed() };
                    assert!(unsafe { extension.get.unwrap()(plugin, index, &mut name) });
                    let text = unsafe { CStr::from_ptr(name.name.as_ptr()) };
                    (text.to_str().unwrap().to_owned(), name.key)
                })
                .collect()
        }

        #[test]
        fn a_songs_names_are_restored_canonical_and_an_older_song_empties_them() {
            let host = host();
            let wrapper: Arc<Wrapper<MxmDrumMachine>> = unsafe { Wrapper::new(&*host) };
            let canonical = |names: &[&str]| {
                let mut all = super::empty();
                for (slot, name) in names.iter().enumerate() {
                    all[slot] = (*name).to_owned();
                }
                serde_json::to_string(&all).unwrap()
            };

            restore(&wrapper, Some("[\"subkick\"]"));
            assert_eq!(
                stored(&wrapper),
                canonical(&["subkick"]),
                "a short list, accepted"
            );
            assert_eq!(names(&wrapper)[0], ("subkick".to_owned(), 36));

            let long = format!("[\" {}\\u0007 \", \"crack\"]", "x".repeat(40));
            restore(&wrapper, Some(&long));
            assert_eq!(
                stored(&wrapper),
                canonical(&[&"x".repeat(super::NAME_CHARS), "crack"]),
                "trimmed and cut, and accepted"
            );

            restore(&wrapper, Some("{\"not\": \"a list\"}"));
            assert_eq!(
                stored(&wrapper),
                canonical(&[]),
                "malformed: emptied, not refused"
            );

            restore(&wrapper, Some("[\"subkick\"]"));
            restore(&wrapper, None);
            assert_eq!(
                stored(&wrapper),
                canonical(&[]),
                "a song from before the names"
            );
            assert_eq!(names(&wrapper)[0], ("bd1".to_owned(), 36));
        }
    }
}
