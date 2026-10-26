//! Each slot's name: the rules, the persistent field, the host's note names and the editor's
//! announcement are mxm-kit's `mxm_drum_shell::slot_names` (the owner, 2026-10-14: "make the
//! software make a 'screen name' like bd1, and then give the user the ability to rename it to
//! anything like bd1 -> subkick"); here, the drum machine's slots as the names read them.

use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::model::{available, made_up_names};

pub use mxm_drum_shell::slot_names::*;

use crate::params::{MxmDrumMachineParams, at, slot_ids};

impl DrumSlots for MxmDrumMachineParams {
    const FIRST_KEY: u8 = crate::FIRST_KEY;

    fn typed_names(&self) -> &RwLock<Vec<String>> {
        &self.slot_names
    }

    fn names_written(&self) -> &AtomicBool {
        &self.names_written
    }

    fn made_up_names(&self) -> [Option<String>; SLOT_COUNT] {
        let models = std::array::from_fn(|slot| self.slots[slot].model_id());
        made_up_names(&models)
    }

    fn plays(&self, slot: usize) -> bool {
        available(self.slots[slot].model_id()).is_some()
    }

    fn midi_channel(&self, slot: usize) -> i32 {
        self.slots[slot].midi_channel.value()
    }

    fn name_dependencies(&self) -> Vec<&'static str> {
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
    use nice_plug::prelude::ClapNoteNames;
    use nice_plug::prelude::ParamSetter;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

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
    fn an_off_slot_names_nothing_to_the_host() {
        let params = Arc::new(MxmDrumMachineParams::default());
        set_model(&params, 3, 0);
        set(&*params, 3, "ghost");
        let names = HostNames(params.clone()).names();
        assert!(names.iter().all(|name| name.key != 39), "Off plays nothing");
        assert_eq!(
            shown(&*params)[3].as_deref(),
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
