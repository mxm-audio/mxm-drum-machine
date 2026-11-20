//! Which outputs the slots are routed to, for the host: mxm-kit's `mxm_drum_shell::outputs` (the
//! MXM CLAP extension `dk.mxm.outputs-in-use/1` and VST3 interface `IOutputsInUse`), read from each
//! slot's Output, so a host can give a slot routed to its own output a track of its own (the owner,
//! 2026-11-19: "if the drum machine opens an individual out, then it makes a group in the daw").

pub use mxm_drum_shell::outputs::*;

use crate::params::{MxmDrumMachineParams, at, slot_ids};

impl DrumOutputs for MxmDrumMachineParams {
    fn output(&self, slot: usize) -> i32 {
        self.slots[slot].output.value()
    }

    fn output_id(&self, slot: usize) -> &'static str {
        slot_ids(slot)[at::OUTPUT]
    }
}

/// The real wrapper, as a host reads it: the outputs in use through the extension, against the
/// ports the full configuration declares.
#[cfg(test)]
mod through_the_wrapper {
    use std::ffi::{CStr, c_char, c_void};
    use std::sync::Arc;

    use clap_sys::ext::audio_ports::{
        CLAP_EXT_AUDIO_PORTS, clap_audio_port_info, clap_plugin_audio_ports,
    };
    use clap_sys::ext::audio_ports_config::{
        CLAP_EXT_AUDIO_PORTS_CONFIG, clap_plugin_audio_ports_config,
    };
    use clap_sys::ext::params::{CLAP_EXT_PARAMS, clap_plugin_params};
    use clap_sys::string_sizes::CLAP_NAME_SIZE;
    use nice_plug::plugin::ParamValue;
    use nice_plug::wrapper::clap::outputs_in_use::{
        DK_MXM_EXT_OUTPUTS_IN_USE, dk_mxm_plugin_outputs_in_use,
    };
    use nice_plug::wrapper::clap::{CLAP_VERSION, Wrapper, clap_host};

    use crate::MxmDrumMachine;
    use mxm_drum_machine_dsp::SLOT_COUNT;

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
            name: c"outputs-test".as_ptr(),
            vendor: c"mxm".as_ptr(),
            url: c"".as_ptr(),
            version: c"1".as_ptr(),
            get_extension: Some(no_extension),
            request_restart: Some(nothing),
            request_process: Some(nothing),
            request_callback: Some(nothing),
        })
    }

    fn extension<T>(wrapper: &Wrapper<MxmDrumMachine>, id: &CStr) -> &'static T {
        let plugin = wrapper.clap_plugin.as_ptr();
        let get_extension = unsafe { (*plugin).get_extension }.unwrap();
        let extension = unsafe { get_extension(plugin, id.as_ptr()) };
        assert!(!extension.is_null(), "{id:?} offered");
        unsafe { &*extension.cast::<T>() }
    }

    fn outputs(wrapper: &Wrapper<MxmDrumMachine>) -> Vec<(u32, String)> {
        let plugin = wrapper.clap_plugin.as_ptr();
        let in_use: &dk_mxm_plugin_outputs_in_use = extension(wrapper, DK_MXM_EXT_OUTPUTS_IN_USE);
        let count = unsafe { in_use.count.unwrap()(plugin) };
        (0..count)
            .map(|index| {
                let mut port = u32::MAX;
                let mut name = [0 as c_char; CLAP_NAME_SIZE];
                assert!(unsafe {
                    in_use.get.unwrap()(plugin, index, &mut port, name.as_mut_ptr())
                });
                let name = unsafe { CStr::from_ptr(name.as_ptr()) };
                (port, name.to_str().unwrap().to_owned())
            })
            .collect()
    }

    /// The output ports of the configuration selected now, by id and name.
    fn ports(wrapper: &Wrapper<MxmDrumMachine>) -> Vec<(u32, String)> {
        let plugin = wrapper.clap_plugin.as_ptr();
        let audio_ports: &clap_plugin_audio_ports = extension(wrapper, CLAP_EXT_AUDIO_PORTS);
        let count = unsafe { audio_ports.count.unwrap()(plugin, false) };
        (0..count)
            .map(|index| {
                let mut info: clap_audio_port_info = unsafe { std::mem::zeroed() };
                assert!(unsafe { audio_ports.get.unwrap()(plugin, index, false, &mut info) });
                let name = unsafe { CStr::from_ptr(info.name.as_ptr()) };
                (info.id, name.to_str().unwrap().to_owned())
            })
            .collect()
    }

    /// Restores a state with these slots' Outputs, as the editor's *All slots* writes them.
    fn route(wrapper: &Wrapper<MxmDrumMachine>, outputs: &[(usize, i32)]) {
        let mut state = wrapper.get_state_object();
        for &(slot, output) in outputs {
            state
                .params
                .insert(format!("output_{}", slot + 1), ParamValue::I32(output));
        }
        wrapper.set_state_object_from_gui(state);
    }

    fn wrapper(host: &clap_host) -> Arc<Wrapper<MxmDrumMachine>> {
        let wrapper: Arc<Wrapper<MxmDrumMachine>> = unsafe { Wrapper::new(host) };
        assert!(unsafe {
            (*wrapper.clap_plugin.as_ptr()).init.unwrap()(wrapper.clap_plugin.as_ptr())
        });
        wrapper
    }

    #[test]
    fn the_init_kit_on_main_uses_no_output() {
        let host = host();
        assert_eq!(outputs(&wrapper(&host)), []);
    }

    #[test]
    fn own_outputs_name_each_slot_on_the_port_the_full_configuration_gives_it() {
        let host = host();
        let wrapper = wrapper(&host);
        let own: Vec<(usize, i32)> = (0..SLOT_COUNT)
            .map(|slot| (slot, slot as i32 + 1))
            .collect();
        route(&wrapper, &own);
        let outputs = outputs(&wrapper);
        assert_eq!(outputs.len(), SLOT_COUNT);
        assert_eq!(outputs[0], (1, "bd1".to_owned()));
        assert_eq!(outputs[1], (2, "sn1".to_owned()));
        // Each id is the full configuration's port for that slot: `Slot 01` is port 1.
        let ports = ports(&wrapper);
        for (slot, (port, _)) in outputs.iter().enumerate() {
            assert_eq!(ports[slot + 1], (*port, format!("Slot {:02}", slot + 1)));
        }
    }

    /// Back to main for port 3 names slots 1 and 2's Outputs, now on output 3, each set to `L+R`.
    #[test]
    fn back_to_main_sets_each_slot_on_the_output_to_l_r() {
        let host = host();
        let wrapper = wrapper(&host);
        route(&wrapper, &[(0, 3), (1, 3), (2, 4)]);
        let plugin = wrapper.clap_plugin.as_ptr();
        let in_use: &dk_mxm_plugin_outputs_in_use = extension(&wrapper, DK_MXM_EXT_OUTPUTS_IN_USE);
        let params: &clap_plugin_params = extension(&wrapper, CLAP_EXT_PARAMS);
        let mut values = Vec::new();
        loop {
            let (mut id, mut value) = (u32::MAX, f64::NAN);
            let index = values.len() as u32;
            if !unsafe { in_use.to_main.unwrap()(plugin, 3, index, &mut id, &mut value) } {
                break;
            }
            values.push((id, value));
        }
        assert_eq!(values.len(), 2, "slots 1 and 2: {values:?}");
        for (id, value) in values {
            assert_eq!(value, 0.0, "L+R");
            let mut now = f64::NAN;
            assert!(unsafe { params.get_value.unwrap()(plugin, id, &mut now) });
            assert_eq!(now, 3.0, "an Output now on output 3");
        }
    }

    #[test]
    fn two_slots_on_one_output_are_named_together_on_the_stereo_configuration_too() {
        let host = host();
        let wrapper = wrapper(&host);
        let plugin = wrapper.clap_plugin.as_ptr();
        let configs: &clap_plugin_audio_ports_config =
            extension(&wrapper, CLAP_EXT_AUDIO_PORTS_CONFIG);
        assert!(
            unsafe { configs.select.unwrap()(plugin, 1) },
            "stereo compatibility"
        );
        assert_eq!(ports(&wrapper).len(), 1);
        route(&wrapper, &[(0, 3), (1, 3)]);
        assert_eq!(outputs(&wrapper), [(3, "bd1 / sn1".to_owned())]);
    }
}

/// The real VST3 wrapper, as a host reads it: `IOutputsInUse` on the component, and each output
/// the bus the full arrangement names after its slot.
#[cfg(test)]
mod through_the_vst3_wrapper {
    use nice_plug::wrapper::vst3::Wrapper;
    use nice_plug::wrapper::vst3::outputs_in_use::IOutputsInUse;
    use nice_plug::wrapper::vst3::vst3::ComWrapper;
    use nice_plug::wrapper::vst3::vst3::Steinberg::Vst::BusDirections_::kOutput;
    use nice_plug::wrapper::vst3::vst3::Steinberg::Vst::MediaTypes_::kAudio;
    use nice_plug::wrapper::vst3::vst3::Steinberg::Vst::{
        BusDirection, BusInfo, IComponentTrait, MediaType,
    };
    use nice_plug::wrapper::vst3::vst3::Steinberg::kResultOk;

    use super::aux_output;
    use crate::MxmDrumMachine;
    use mxm_drum_machine_dsp::SLOT_COUNT;

    #[test]
    fn it_says_which_outputs_and_each_is_the_bus_named_after_its_slot() {
        let wrapper = ComWrapper::new(Wrapper::<MxmDrumMachine>::new());
        let in_use = wrapper.to_com_ptr::<IOutputsInUse>().expect("offered");
        let ptr = in_use.as_ptr();
        let mut count = -1;
        assert_eq!(
            unsafe { ((*(*ptr).vtbl).getOutputCount)(ptr, &mut count) },
            kResultOk,
            "it says"
        );
        assert_eq!(count, 0, "the Init kit on main");
        for aux in 0..SLOT_COUNT {
            let mut info: BusInfo = unsafe { std::mem::zeroed() };
            let bus = aux_output(aux) as i32;
            assert_eq!(
                unsafe {
                    wrapper.getBusInfo(kAudio as MediaType, kOutput as BusDirection, bus, &mut info)
                },
                kResultOk
            );
            let end = info.name.iter().position(|&unit| unit == 0).unwrap();
            let name = String::from_utf16(&info.name[..end]).unwrap();
            assert_eq!(name, format!("Slot {:02}", aux + 1));
        }
    }
}
