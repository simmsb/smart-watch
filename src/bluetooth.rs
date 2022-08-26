use core::ffi::{c_char, c_int};
use std::cell::UnsafeCell;
use std::ffi::{c_void, CStr};
use std::ptr::null_mut;
use std::sync::atomic::AtomicU8;
use std::sync::{Mutex, Arc, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use color_eyre::eyre::eyre;
use crossbeam::channel;
use esp_idf_svc::eventloop::EspEventFetchData;
use esp_idf_sys::{
    ble_gap_adv_params, ble_gap_adv_set_fields, ble_gap_adv_start, ble_gap_conn_desc,
    ble_gap_conn_find, ble_gap_event, ble_gatt_access_ctxt, ble_gatt_chr_def,
    ble_gatt_register_ctxt, ble_gatt_svc_def, ble_gattc_notify_custom, ble_gatts_add_svcs,
    ble_gatts_count_cfg, ble_hs_adv_fields, ble_hs_cfg, ble_hs_id_copy_addr, ble_hs_id_infer_auto,
    ble_hs_mbuf_from_flat, ble_hs_mbuf_to_flat, ble_hs_util_ensure_addr, ble_store_util_status_rr,
    ble_uuid128_t, ble_uuid16_t, ble_uuid_cmp, ble_uuid_t, ble_uuid_to_str, esp,
    esp_nimble_hci_and_controller_deinit, esp_nimble_hci_and_controller_init,
    nimble_port_freertos_deinit, nimble_port_freertos_init, nimble_port_init, nimble_port_run,
    os_mbuf, os_mbuf_append, strlen, BLE_ATT_ERR_INSUFFICIENT_RES, BLE_GAP_CONN_MODE_UND,
    BLE_GAP_DISC_MODE_GEN, BLE_GAP_EVENT_ADV_COMPLETE, BLE_GAP_EVENT_CONNECT,
    BLE_GAP_EVENT_CONN_UPDATE, BLE_GAP_EVENT_DISCONNECT, BLE_GAP_EVENT_MTU,
    BLE_GATT_ACCESS_OP_READ_CHR, BLE_GATT_ACCESS_OP_WRITE_CHR, BLE_GATT_CHR_F_NOTIFY,
    BLE_GATT_CHR_F_READ, BLE_GATT_CHR_F_WRITE, BLE_GATT_REGISTER_OP_CHR, BLE_GATT_REGISTER_OP_DSC,
    BLE_GATT_REGISTER_OP_SVC, BLE_GATT_SVC_TYPE_PRIMARY, BLE_HS_ADV_F_BREDR_UNSUP,
    BLE_HS_ADV_F_DISC_GEN, BLE_HS_ADV_TX_PWR_LVL_AUTO, BLE_UUID_STR_LEN, BLE_UUID_TYPE_128,
    BLE_UUID_TYPE_16, CONFIG_BT_NIMBLE_MAX_CONNECTIONS, ble_hs_stop, ble_hs_stop_listener, nimble_port_stop, nimble_port_deinit,
};
use once_cell::sync::{Lazy, OnceCell};
use prost::Message;
use tracing::{error, info};

use crate::axp192::BATTERY_PERCENT;
use crate::message;

pub static QUEUE: Lazy<(
    channel::Sender<message::Notification>,
    channel::Receiver<message::Notification>,
)> = Lazy::new(|| channel::bounded(4));

static TX_THREAD: OnceCell<JoinHandle<()>> = OnceCell::new();

static CONN_COUNT: AtomicU8 = AtomicU8::new(0);

pub fn ble_connected() -> bool {
    let n = CONN_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    info!(n, "Ongoing bluetooth connections");
    n > 0
}

const BLE_UUID_TYPE_128_: ble_uuid_t = ble_uuid_t {
    type_: BLE_UUID_TYPE_128 as u8,
};

const BLE_UUID_TYPE_16_: ble_uuid_t = ble_uuid_t {
    type_: BLE_UUID_TYPE_16 as u8,
};

const fn inv(
    [a_0, a_1, a_2, a_3, b_0, b_1, b_2, b_3, c_0, c_1, c_2, c_3, d_0, d_1, d_2, d_3]: [u8; 16],
) -> [u8; 16] {
    [
        d_3, d_2, d_1, d_0, c_3, c_2, c_1, c_0, b_3, b_2, b_1, b_0, a_3, a_2, a_1, a_0,
    ]
}

static mut BLE_BAT_SERVICE: ble_uuid16_t = ble_uuid16_t {
    u: BLE_UUID_TYPE_16_,
    value: 0x180f,
};

static mut BLE_BAT_CHAR: ble_uuid16_t = ble_uuid16_t {
    u: BLE_UUID_TYPE_16_,
    value: 0x2A19,
};

static mut BLE_DATA_IN_SERVICE: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: inv(*uuid::uuid!("98200001-2160-4474-82b4-1a25cef92156").as_bytes()),
};

static mut BLE_DATA_IN_CHAR: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: inv(*uuid::uuid!("98200002-2160-4474-82b4-1a25cef92156").as_bytes()),
};

static mut BLE_DATA_OUT_CHAR: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: inv(*uuid::uuid!("98200003-2160-4474-82b4-1a25cef92156").as_bytes()),
};

static mut BLE_DATA_IN_HANDLE: UnsafeCell<u16> = UnsafeCell::new(0);
static mut BLE_DATA_OUT_HANDLE: UnsafeCell<u16> = UnsafeCell::new(0);
static mut BLE_LE_BAT_CHAR_HANDLE: UnsafeCell<u16> = UnsafeCell::new(0);

static mut GATT_SERVICES: [esp_idf_sys::ble_gatt_svc_def; 3] = unsafe {
    [
        ble_gatt_svc_def {
            type_: BLE_GATT_SVC_TYPE_PRIMARY as u8,
            uuid: &BLE_BAT_SERVICE.u,
            characteristics: &[
                ble_gatt_chr_def {
                    uuid: &BLE_BAT_CHAR.u,
                    access_cb: Some(ble_batt_handler),
                    val_handle: BLE_LE_BAT_CHAR_HANDLE.get(),
                    flags: (BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY) as u16,
                    arg: std::ptr::null_mut(),
                    descriptors: std::ptr::null_mut(),
                    min_key_size: 0,
                },
                const_zero::const_zero!(ble_gatt_chr_def),
            ] as *const _,
            includes: std::ptr::null_mut(),
        },
        ble_gatt_svc_def {
            type_: BLE_GATT_SVC_TYPE_PRIMARY as u8,
            uuid: &BLE_DATA_IN_SERVICE.u,
            characteristics: &[
                ble_gatt_chr_def {
                    uuid: &BLE_DATA_IN_CHAR.u,
                    access_cb: Some(ble_data_in_handler),
                    val_handle: BLE_DATA_IN_HANDLE.get(),
                    flags: BLE_GATT_CHR_F_WRITE as u16,
                    arg: std::ptr::null_mut(),
                    descriptors: std::ptr::null_mut(),
                    min_key_size: 0,
                },
                ble_gatt_chr_def {
                    uuid: &BLE_DATA_OUT_CHAR.u,
                    access_cb: Some(ble_data_in_handler),
                    val_handle: BLE_DATA_OUT_HANDLE.get(),
                    flags: BLE_GATT_CHR_F_NOTIFY as u16,
                    arg: std::ptr::null_mut(),
                    descriptors: std::ptr::null_mut(),
                    min_key_size: 0,
                },
                const_zero::const_zero!(ble_gatt_chr_def),
            ] as *const _,
            includes: std::ptr::null_mut(),
        },
        const_zero::const_zero!(ble_gatt_svc_def),
    ]
};

unsafe extern "C" fn ble_spp_server_on_reset(reason: c_int) {
    info!(reason, "Resetting ble");
}

static mut OWN_ADDR_TYPE: u8 = 0;

unsafe extern "C" fn ble_spp_server_on_sync() {
    let rc = ble_hs_util_ensure_addr(0);
    assert_eq!(rc, 0, "ble_hs_util_ensure_addr");

    let rc = ble_hs_id_infer_auto(0, &mut OWN_ADDR_TYPE as *mut _);
    if rc != 0 {
        error!(rc, "Failed to determine address type");
        return;
    }

    let mut addr_val = [0u8; 6];
    ble_hs_id_copy_addr(OWN_ADDR_TYPE, &mut addr_val as *mut _, std::ptr::null_mut());

    info!(device_address = ?addr_val, "Found device address");

    ble_spp_server_advertise();
}

unsafe extern "C" fn ble_batt_handler(
    _conn_handle: u16,
    _attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    _arg: *mut c_void,
) -> i32 {
    let ctxt_ = *ctxt;

    let uuid = (*ctxt_.__bindgen_anon_1.chr).uuid;

    assert_eq!(ble_uuid_cmp(uuid, &BLE_BAT_CHAR.u), 0);
    assert_eq!(ctxt_.op as u32, BLE_GATT_ACCESS_OP_READ_CHR);

    let batt_level = BATTERY_PERCENT.load(std::sync::atomic::Ordering::Relaxed);
    info!(batt_level, "Reading battery level");

    let rc = os_mbuf_append(
        ctxt_.om,
        &batt_level as *const u8 as *const _,
        std::mem::size_of::<u8>() as u16,
    );
    if rc != 0 {
        return BLE_ATT_ERR_INSUFFICIENT_RES as i32;
    }

    0
}

unsafe extern "C" fn ble_data_in_handler(
    conn_handle: u16,
    attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    _arg: *mut c_void,
) -> i32 {
    let ctxt_ = *ctxt;
    match ctxt_.op as u32 {
        BLE_GATT_ACCESS_OP_WRITE_CHR => {
            let len = (*ctxt_.om).om_len;

            info!(
                conn_handle,
                attr_handle, len, "Data received in write event"
            );

            let mut buf = [0u8; 256];
            let mut out_len = 0u16;
            let rc = ble_hs_mbuf_to_flat(
                ctxt_.om,
                &mut buf as *mut u8 as *mut _,
                std::mem::size_of::<[u8; 256]>() as u16,
                &mut out_len as *mut _,
            );
            if rc != 0 {
                error!("Couldn't fetch mbuf in write handler");
                return 0;
            }
            let buf = &buf[..out_len as usize];

            let msg = match message::Message::decode(buf)
                .map_err(|err| eyre!("Failed to decode message: {:?}", err))
                .and_then(message::validate_msg)
            {
                Ok(msg) => msg,
                Err(err) => {
                    error!(?err, "While decoding a message");
                    return 0;
                }
            };

            info!(?msg, "Got message");

            if let Err(err) = message::push_message(msg) {
                error!(?err, "Failed to push message");
            }
        }
        _ => {}
    }

    0
}

pub fn ble_spp_server_advertise() {
    // if we're exposing this we should prevent concurrent usage
    // even this might not be safe, we should probably be doing a channel
    static ADV_MUTEX: Mutex<()> = Mutex::new(());
    let _handle = ADV_MUTEX.lock().unwrap();
    unsafe {
        let name = ble_svc_gap_device_name();
        static UUIDS16: &[ble_uuid16_t] = &[unsafe { BLE_BAT_SERVICE }];
        // static UUIDS128: &[ble_uuid128_t] = &[unsafe { BLE_LE_NRF_SERVICE }];

        let mut fields = ble_hs_adv_fields {
            flags: (BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP) as u8,
            tx_pwr_lvl: BLE_HS_ADV_TX_PWR_LVL_AUTO as i8,
            name: name as *const _,
            name_len: strlen(name) as u8,
            uuids16: UUIDS16.as_ptr(),
            num_uuids16: UUIDS16.len() as u8,
            adv_itvl: 8000,
            // uuids128: UUIDS128.as_ptr(),
            // num_uuids128: UUIDS128.len() as u8,
            ..Default::default()
        };

        fields.set_tx_pwr_lvl_is_present(1);
        fields.set_name_is_complete(1);
        fields.set_uuids16_is_complete(1);
        fields.set_adv_itvl_is_present(1);

        let rc = ble_gap_adv_set_fields(&fields);
        if rc != 0 {
            error!(rc, "error setting advertisement data");
            return;
        }

        let adv_params = ble_gap_adv_params {
            conn_mode: BLE_GAP_CONN_MODE_UND as u8,
            disc_mode: BLE_GAP_DISC_MODE_GEN as u8,
            ..Default::default()
        };

        let duration_ms = Duration::from_secs(60).as_millis() as i32;
        let rc = ble_gap_adv_start(
            OWN_ADDR_TYPE,
            std::ptr::null(),
            duration_ms,
            &adv_params,
            Some(ble_spp_server_gap_event),
            std::ptr::null_mut(),
        );
        if rc != 0 {
            error!(rc, "error enabling advertisement");
            return;
        }

        info!("started to advertise");
    }
}

unsafe extern "C" fn ble_spp_server_gap_event(event: *mut ble_gap_event, _arg: *mut c_void) -> i32 {
    let event_ = *event;
    let mut desc = ble_gap_conn_desc::default();

    match event_.type_ as u32 {
        BLE_GAP_EVENT_CONNECT => {
            let connect = event_.__bindgen_anon_1.connect;
            info!(
                status = connect.status,
                "connection {}",
                if connect.status == 0 {
                    "established"
                } else {
                    "failed"
                }
            );

            if connect.status == 0 {
                let rc = ble_gap_conn_find(connect.conn_handle, &mut desc as *mut _);
                assert_eq!(rc, 0, "ble_gap_conn_find");
                info!(handle = connect.conn_handle, ?desc, "Conn desc");
                CONNECTION_HANDLES[connect.conn_handle as usize] = connect.conn_handle;
                CONN_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            if connect.status != 0 {
                ble_spp_server_advertise();
            }
        }

        BLE_GAP_EVENT_DISCONNECT => {
            let disconnect = event_.__bindgen_anon_1.disconnect;
            info!(reason = disconnect.reason, "Disconnect");
            CONN_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }

        BLE_GAP_EVENT_CONN_UPDATE => {
            let conn_update = event_.__bindgen_anon_1.conn_update;
            info!(status = conn_update.status, "Connection update");
            let rc = ble_gap_conn_find(conn_update.conn_handle, &mut desc as *mut _);
            assert_eq!(rc, 0, "ble_gap_conn_find");
            info!(?desc, "Conn desc");
        }

        BLE_GAP_EVENT_ADV_COMPLETE => {
            let adv_complete = event_.__bindgen_anon_1.adv_complete;
            info!(reason = adv_complete.reason, "advertise complete");
        }

        BLE_GAP_EVENT_MTU => {
            let mtu = event_.__bindgen_anon_1.mtu;
            info!(
                conn_handle = mtu.conn_handle,
                channel_id = mtu.channel_id,
                value = mtu.value,
                "mtu update"
            );
        }

        _ => {}
    }

    0
}
// sepples moment

unsafe extern "C" fn gatt_svr_register_cb(ctxt: *mut ble_gatt_register_ctxt, _arg: *mut c_void) {
    let mut buf = [0 as c_char; BLE_UUID_STR_LEN as usize];
    let ctxt_ = *ctxt;

    match ctxt_.op as u32 {
        BLE_GATT_REGISTER_OP_SVC => {
            let svc = ctxt_.__bindgen_anon_1.svc;
            let uuid = CStr::from_ptr(ble_uuid_to_str((*svc.svc_def).uuid, &mut buf as *mut _));
            info!(uuid = ?uuid, handle = ?svc.handle, "Registering gatt service");
        }
        BLE_GATT_REGISTER_OP_CHR => {
            let chr = ctxt_.__bindgen_anon_1.chr;
            let uuid = CStr::from_ptr(ble_uuid_to_str((*chr.chr_def).uuid, &mut buf as *mut _));
            info!(uuid = ?uuid, def_handle = ?chr.def_handle, val_handle = ?chr.val_handle, "Registering characteristic");
        }
        BLE_GATT_REGISTER_OP_DSC => {
            let dsc = ctxt_.__bindgen_anon_1.dsc;
            let uuid = CStr::from_ptr(ble_uuid_to_str((*dsc.dsc_def).uuid, &mut buf as *mut _));
            info!(uuid = ?uuid, handle = ?dsc.handle, "Registering gatt service");
        }
        _ => {}
    }
}

fn tx_thread() {
    let receiver = QUEUE.1.clone();

    for msg in receiver {
        let att_handle = unsafe { *BLE_DATA_OUT_HANDLE.get() };
        let buf = msg.encode_to_vec();
        for &handle in unsafe { &CONNECTION_HANDLES } {
            let txom: *mut os_mbuf =
                unsafe { ble_hs_mbuf_from_flat(buf.as_ptr() as *const _, buf.len() as u16) };

            let rc = unsafe { ble_gattc_notify_custom(handle, att_handle, txom) };
            if rc == 0 {
                info!("Sent notif");
            } else {
                info!(rc, "Error sending notif");
            }
        }
    }
}

static mut CONNECTION_HANDLES: [u16; CONFIG_BT_NIMBLE_MAX_CONNECTIONS as usize] =
    [0; CONFIG_BT_NIMBLE_MAX_CONNECTIONS as usize];

unsafe extern "C" fn ble_spp_server_host_task(_param: *mut c_void) {
    info!("BLE host task started");

    nimble_port_run();
    nimble_port_freertos_deinit();
}

unsafe extern "C" fn stop_fn(_status: c_int, arg: *mut c_void) {
    info!("Bluetooth hs seems to have stopped");
    // just serves to drop the handle
    let _h = Arc::from_raw(arg as *mut _ as *const MutexGuard<()>);
}

pub fn stop_ble() -> color_eyre::Result<()> {
    static STOP_LOCK: Mutex<()> = Mutex::new(());
    static STOPPED_LOCK: Mutex<()> = Mutex::new(());

    let _h = STOP_LOCK.lock().unwrap();
    let h = STOPPED_LOCK.lock().unwrap();
    let h = Arc::into_raw(Arc::new(h));

    unsafe {
        let mut listener = ble_hs_stop_listener::default();
        if let Err(e) = esp!(ble_hs_stop(&mut listener, Some(stop_fn), h as *const _ as *mut _)) {
            std::mem::drop(Arc::from_raw(h));
            return Err(e)?;
        }
    }

    info!("Dispatched stop request");

    // ok, now we try to lock the inner lock again, it *should* unlock when stop_fn runs
    let _h2 = STOPPED_LOCK.lock().unwrap();

    // now we can call the other deinit functions?

    unsafe {
        esp!(nimble_port_stop())?;
        nimble_port_deinit();
        esp!(esp_nimble_hci_and_controller_deinit())?
    }


    Ok(())
}

pub fn init_ble() -> color_eyre::Result<()> {
    unsafe {
        if let Err(err) = esp!(esp_idf_sys::nvs_flash_init()) {
            if err.code() == esp_idf_sys::ESP_ERR_NVS_NO_FREE_PAGES
                || err.code() == esp_idf_sys::ESP_ERR_NVS_NEW_VERSION_FOUND
            {
                esp!(esp_idf_sys::nvs_flash_erase())?;
                esp!(esp_idf_sys::nvs_flash_init())?;
            }
        }

        info!("Initializing bluetooth");

        esp!(esp_nimble_hci_and_controller_init())?;

        nimble_port_init();

        ble_hs_cfg.reset_cb = Some(ble_spp_server_on_reset);
        ble_hs_cfg.sync_cb = Some(ble_spp_server_on_sync);
        ble_hs_cfg.gatts_register_cb = Some(gatt_svr_register_cb);
        ble_hs_cfg.store_status_cb = Some(ble_store_util_status_rr);
        ble_hs_cfg.sm_io_cap = 3;
        ble_hs_cfg.set_sm_bonding(1);
        ble_hs_cfg.set_sm_sc(1);
        ble_hs_cfg.sm_our_key_dist = 1;
        ble_hs_cfg.sm_their_key_dist = 1;

        // gatt_svr_init
        ble_svc_gap_init();
        ble_svc_gatt_init();
        esp!(ble_gatts_count_cfg(&GATT_SERVICES as *const _))?;
        esp!(ble_gatts_add_svcs(&GATT_SERVICES as *const _))?;
        esp!(ble_svc_gap_device_name_set(
            b"\xF0\x9F\xA6\x80\0" as *const u8 as *const _
        ))?;
        ble_store_config_init();
        nimble_port_freertos_init(Some(ble_spp_server_host_task));
    }

    TX_THREAD.get_or_init(|| std::thread::spawn(|| tx_thread()));

    Ok(())
}

extern "C" {
    fn ble_svc_gap_init();
    fn ble_svc_gatt_init();
    fn ble_svc_gap_device_name() -> *const c_char;
    fn ble_svc_gap_device_name_set(name: *const c_char) -> c_int;
    fn ble_store_config_init();
}
