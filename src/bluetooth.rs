use core::ffi::c_char;
use std::ffi::{c_void, CStr};
use std::ptr;

use esp_idf_hal::serial::Uart;
use esp_idf_sys::{
    ble_gatt_access_ctxt, ble_gatt_chr_def, ble_gatt_register_ctxt, ble_gatt_svc_def,
    ble_gatts_add_svcs, ble_gatts_count_cfg, ble_hs_mbuf_to_flat, ble_uuid128_t, ble_uuid_cmp,
    ble_uuid_t, ble_uuid_to_str, esp, esp_nofail, nimble_port_init, os_mbuf, os_mbuf_append, rand,
    uart_config_t, uart_driver_install, uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_RTS,
    uart_param_config, uart_parity_t_UART_PARITY_DISABLE, uart_set_pin,
    uart_stop_bits_t_UART_STOP_BITS_1, uart_word_length_t_UART_DATA_8_BITS, EspError,
    QueueHandle_t, BLE_ATT_ERR_INSUFFICIENT_RES, BLE_ATT_ERR_INVALID_ATTR_VALUE_LEN,
    BLE_ATT_ERR_UNLIKELY, BLE_GATT_ACCESS_OP_READ_CHR, BLE_GATT_ACCESS_OP_WRITE_CHR,
    BLE_GATT_CHR_F_READ, BLE_GATT_CHR_F_READ_ENC, BLE_GATT_CHR_F_WRITE, BLE_GATT_CHR_F_WRITE_ENC,
    BLE_GATT_REGISTER_OP_CHR, BLE_GATT_REGISTER_OP_DSC, BLE_GATT_REGISTER_OP_SVC,
    BLE_GATT_SVC_TYPE_PRIMARY, BLE_UUID_STR_LEN, BLE_UUID_TYPE_128, UART_PIN_NO_CHANGE,
};
use tracing::{debug, error, info};

const BLE_UUID_TYPE_128_: ble_uuid_t = ble_uuid_t {
    type_: BLE_UUID_TYPE_128 as u8,
};

static mut GATT_SVR_SVC_SEC_TEST_UUID: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: [
        0x2d, 0x71, 0xa2, 0x59, 0xb4, 0x58, 0xc8, 0x12, 0x99, 0x99, 0x43, 0x95, 0x12, 0x2f, 0x46,
        0x59,
    ],
};

static mut GATT_SVR_SVC_SEC_TEST_RAND_UUID: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: [
        0xf6, 0x6d, 0xc9, 0x07, 0x71, 0x00, 0x16, 0xb0, 0xe1, 0x45, 0x7e, 0x89, 0x9e, 0x65, 0x3a,
        0x5c,
    ],
};

static mut GATT_SVR_SVC_SEC_TEST_STATIC_UUID: ble_uuid128_t = ble_uuid128_t {
    u: BLE_UUID_TYPE_128_,
    value: [
        0xf7, 0x6d, 0xc9, 0x07, 0x71, 0x00, 0x16, 0xb0, 0xe1, 0x45, 0x7e, 0x89, 0x9e, 0x65, 0x3a,
        0x5c,
    ],
};

static mut GATT_SVR_SVC_SEC_TEST_STATIC_VAL: u8 = 0;

static mut GATT_SECURITY_SERVICES: [esp_idf_sys::ble_gatt_svc_def; 2] = unsafe {
    [
        ble_gatt_svc_def {
            type_: BLE_GATT_SVC_TYPE_PRIMARY as u8,
            uuid: &GATT_SVR_SVC_SEC_TEST_UUID.u,
            characteristics: &[
                ble_gatt_chr_def {
                    uuid: &GATT_SVR_SVC_SEC_TEST_RAND_UUID.u,
                    access_cb: Some(gatt_svr_chr_access_sec_test),
                    flags: (BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_READ_ENC) as u16,
                    arg: std::ptr::null_mut(),
                    descriptors: std::ptr::null_mut(),
                    min_key_size: 0,
                    val_handle: std::ptr::null_mut(),
                },
                ble_gatt_chr_def {
                    uuid: &GATT_SVR_SVC_SEC_TEST_STATIC_UUID.u,
                    access_cb: Some(gatt_svr_chr_access_sec_test),
                    flags: (BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_WRITE_ENC) as u16,
                    arg: std::ptr::null_mut(),
                    descriptors: std::ptr::null_mut(),
                    min_key_size: 0,
                    val_handle: std::ptr::null_mut(),
                },
                const_zero::const_zero!(ble_gatt_chr_def),
            ] as *const _,
            includes: std::ptr::null_mut(),
        },
        const_zero::const_zero!(ble_gatt_svc_def),
    ]
};

// sepples moment

unsafe fn gatt_svr_chr_write(
    om: *mut os_mbuf,
    min_len: u16,
    max_len: u16,
    dst: *mut c_void,
    len: *mut u16,
) -> i32 {
    let len_ = (*om).om_len;

    if len_ < min_len || len_ > max_len {
        return BLE_ATT_ERR_INVALID_ATTR_VALUE_LEN as i32;
    }

    let rc = ble_hs_mbuf_to_flat(om, dst, max_len, len);
    if rc != 0 {
        return BLE_ATT_ERR_UNLIKELY as i32;
    }

    0
}

unsafe extern "C" fn gatt_svr_chr_access_sec_test(
    conn_handle: u16,
    attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    arg: *mut c_void,
) -> i32 {
    let uuid = (*(*ctxt).__bindgen_anon_1.chr).uuid;

    if ble_uuid_cmp(uuid, &GATT_SVR_SVC_SEC_TEST_RAND_UUID.u) == 0 {
        let r = rand();
        let rc = os_mbuf_append(
            (*ctxt).om,
            &r as *const i32 as *const _,
            std::mem::size_of_val(&r) as u16,
        );

        if rc == 0 {
            return 0;
        } else {
            return BLE_ATT_ERR_INSUFFICIENT_RES as i32;
        }
    }

    if ble_uuid_cmp(uuid, &GATT_SVR_SVC_SEC_TEST_STATIC_UUID.u) == 0 {
        match (*ctxt).op as u32 {
            BLE_GATT_ACCESS_OP_READ_CHR => {
                let rc = os_mbuf_append(
                    (*ctxt).om,
                    &GATT_SVR_SVC_SEC_TEST_STATIC_VAL as *const u8 as *const _,
                    std::mem::size_of::<u8>() as u16,
                );
                if rc == 0 {
                    return 0;
                } else {
                    return BLE_ATT_ERR_INSUFFICIENT_RES as i32;
                }
            }
            BLE_GATT_ACCESS_OP_WRITE_CHR => {
                let rc = gatt_svr_chr_write(
                    (*ctxt).om,
                    std::mem::size_of::<u8>() as u16,
                    std::mem::size_of::<u8>() as u16,
                    &mut GATT_SVR_SVC_SEC_TEST_STATIC_VAL as *mut u8 as *mut _,
                    std::ptr::null_mut(),
                );
                return rc;
            }
            _ => {
                return BLE_ATT_ERR_UNLIKELY as i32;
            }
        }
    }

    0
}

unsafe extern "C" fn gatt_server_register_cb(ctxt: *mut ble_gatt_register_ctxt, arg: *mut c_void) {
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

static mut SPP_COMMON_QUEUE_HANDLE: QueueHandle_t = std::ptr::null_mut();

unsafe fn ble_uart_init<U: Uart>(_uart: U) -> color_eyre::Result<()> {
    let uart_config = uart_config_t {
        baud_rate: 115200,
        data_bits: uart_word_length_t_UART_DATA_8_BITS,
        parity: uart_parity_t_UART_PARITY_DISABLE,
        stop_bits: uart_stop_bits_t_UART_STOP_BITS_1,
        flow_ctrl: uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_RTS,
        rx_flow_ctrl_thresh: 122,
        ..uart_config_t::default()
    };

    esp!(uart_param_config(U::port(), &uart_config))?;
    esp!(uart_set_pin(
        U::port(),
        UART_PIN_NO_CHANGE,
        UART_PIN_NO_CHANGE,
        UART_PIN_NO_CHANGE,
        UART_PIN_NO_CHANGE
    ))?;

    esp!(uart_driver_install(
        U::port(),
        4096,
        8192,
        10,
        &mut SPP_COMMON_QUEUE_HANDLE,
        0
    ))?;

    // TODO: https://github.com/espressif/esp-idf/blob/master/examples/bluetooth/nimble/ble_spp/spp_server/main/main.c#L383

    Ok(())
}

pub fn init_ble(uart: impl Uart) -> color_eyre::Result<()> {
    unsafe {
        if let Err(err) = esp!(esp_idf_sys::nvs_flash_init()) {
            if err.code() == esp_idf_sys::ESP_ERR_NVS_NO_FREE_PAGES
                || err.code() == esp_idf_sys::ESP_ERR_NVS_NEW_VERSION_FOUND
            {
                esp!(esp_idf_sys::nvs_flash_erase())?;
                esp!(esp_idf_sys::nvs_flash_init())?;
            }
        }

        nimble_port_init();

        ble_uart_init(uart);

        ble_svc_gap_init();
        ble_svc_gatt_init();
        esp!(ble_gatts_count_cfg(&GATT_SECURITY_SERVICES as *const _))?;
        esp!(ble_gatts_add_svcs(&GATT_SECURITY_SERVICES as *const _))?;
    }

    Ok(())
}

extern "C" {
    fn ble_svc_gap_init();
    fn ble_svc_gatt_init();
}

// pub fn init_ble() -> color_eyre::Result<()> {
//     let mut config = esp_idf_sys::esp_bt_controller_config_t {
//         controller_task_stack_size: esp_idf_sys::ESP_TASK_BT_CONTROLLER_STACK as u16,
//         controller_task_prio: esp_idf_sys::ESP_TASK_BT_CONTROLLER_PRIO as u8,
//         hci_uart_no: esp_idf_sys::BT_HCI_UART_NO_DEFAULT as u8,
//         hci_uart_baudrate: esp_idf_sys::BT_HCI_UART_BAUDRATE_DEFAULT,
//         scan_duplicate_mode: esp_idf_sys::SCAN_DUPLICATE_MODE as u8,
//         scan_duplicate_type: esp_idf_sys::SCAN_DUPLICATE_TYPE_VALUE as u8,
//         normal_adv_size: esp_idf_sys::NORMAL_SCAN_DUPLICATE_CACHE_SIZE as u16,
//         mesh_adv_size: esp_idf_sys::MESH_DUPLICATE_SCAN_CACHE_SIZE as u16,
//         send_adv_reserved_size: esp_idf_sys::SCAN_SEND_ADV_RESERVED_SIZE as u16,
//         controller_debug_flag: esp_idf_sys::CONTROLLER_ADV_LOST_DEBUG_BIT,
//         mode: esp_idf_sys::esp_bt_mode_t_ESP_BT_MODE_BLE as u8,
//         ble_max_conn: esp_idf_sys::CONFIG_BTDM_CTRL_BLE_MAX_CONN_EFF as u8,
//         bt_max_acl_conn: esp_idf_sys::CONFIG_BTDM_CTRL_BR_EDR_MAX_ACL_CONN_EFF as u8,
//         bt_sco_datapath: esp_idf_sys::CONFIG_BTDM_CTRL_BR_EDR_SCO_DATA_PATH_EFF as u8,
//         auto_latency: esp_idf_sys::BTDM_CTRL_AUTO_LATENCY_EFF != 0,
//         bt_legacy_auth_vs_evt: esp_idf_sys::BTDM_CTRL_LEGACY_AUTH_VENDOR_EVT_EFF != 0,
//         bt_max_sync_conn: esp_idf_sys::CONFIG_BTDM_CTRL_BR_EDR_MAX_SYNC_CONN_EFF as u8,
//         ble_sca: esp_idf_sys::CONFIG_BTDM_BLE_SLEEP_CLOCK_ACCURACY_INDEX_EFF as u8,
//         pcm_role: esp_idf_sys::CONFIG_BTDM_CTRL_PCM_ROLE_EFF as u8,
//         pcm_polar: esp_idf_sys::CONFIG_BTDM_CTRL_PCM_POLAR_EFF as u8,
//         hli: esp_idf_sys::BTDM_CTRL_HLI != 0,
//         magic: esp_idf_sys::ESP_BT_CONTROLLER_CONFIG_MAGIC_VAL,
//     };

//     unsafe {
//         if let Err(err) = esp!(esp_idf_sys::nvs_flash_init()) {
//             if err.code() == esp_idf_sys::ESP_ERR_NVS_NO_FREE_PAGES
//                 || err.code() == esp_idf_sys::ESP_ERR_NVS_NEW_VERSION_FOUND
//             {
//                 esp!(esp_idf_sys::nvs_flash_erase())?;
//                 esp!(esp_idf_sys::nvs_flash_init())?;
//             }
//         }

//         esp!(esp_idf_sys::esp_bt_controller_mem_release(
//             esp_idf_sys::esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT
//         ))?;
//         esp!(esp_idf_sys::esp_bt_controller_init(&mut config))?;
//         esp!(esp_idf_sys::esp_bt_controller_enable(
//             esp_idf_sys::esp_bt_mode_t_ESP_BT_MODE_BLE
//         ))?;
//         esp!(esp_idf_sys::esp_bluedroid_init())?;
//         esp!(esp_idf_sys::esp_bluedroid_enable())?;
//         esp!(esp_idf_sys::esp_ble_gatts_register_callback(Some(
//             ble_gatts_callback
//         )))?;
//         esp!(esp_idf_sys::esp_ble_gap_register_callback(Some(
//             ble_gap_callback
//         )))?;
//         esp!(esp_idf_sys::esp_ble_gatts_app_register(APP_ID))?;
//         esp!(esp_idf_sys::esp_ble_gatt_set_local_mtu(512))?;
//     }
//     // esp!()
//     //
//     Ok(())
// }

// struct GattsProfile {
//     if_: esp_idf_sys::esp_gatt_if_t,
//     service_id: esp_idf_sys::esp_gatt_srvc_id_t,
//     service_handle: u16,
//     char_uuid: esp_idf_sys::esp_bt_uuid_t,
//     char_handle: u16,
//     descr_uuid: esp_idf_sys::esp_bt_uuid_t,
//     descr_handle: u16,
//     conn_id: u16,
// }

// static mut GL_PROFILE: GattsProfile = GattsProfile {
//     if_: 0,
//     service_id: esp_idf_sys::esp_gatt_srvc_id_t {
//         id: esp_idf_sys::esp_gatt_id_t {
//             uuid: esp_idf_sys::esp_bt_uuid_t {
//                 len: 0,
//                 uuid: esp_idf_sys::esp_bt_uuid_t__bindgen_ty_1 { uuid128: [0; 16] },
//             },
//             inst_id: 0,
//         },
//         is_primary: false,
//     },
//     service_handle: 0,
//     char_uuid: esp_idf_sys::esp_bt_uuid_t {
//         len: 0,
//         uuid: esp_idf_sys::esp_bt_uuid_t__bindgen_ty_1 { uuid128: [0; 16] },
//     },
//     char_handle: 0,
//     descr_uuid: esp_idf_sys::esp_bt_uuid_t {
//         len: 0,
//         uuid: esp_idf_sys::esp_bt_uuid_t__bindgen_ty_1 { uuid128: [0; 16] },
//     },
//     descr_handle: 0,
//     conn_id: 0,
// };

// unsafe extern "C" fn ble_gatts_callback(
//     event: esp_idf_sys::esp_gatts_cb_event_t,
//     gatts_if: esp_idf_sys::esp_gatt_if_t,
//     param: *mut esp_idf_sys::esp_ble_gatts_cb_param_t,
// ) {
//     let param_ = *param;

//     match event {
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_REG_EVT => {
//             if param_.reg.status == esp_idf_sys::esp_gatt_status_t_ESP_GATT_OK {
//                 GL_PROFILE.if_ = gatts_if;
//             } else {
//                 error!(status = ?param_.reg, "Reg app failed");
//                 return;
//             }
//         }
//         _ => {}
//     }

//     if (gatts_if as u32) == esp_idf_sys::ESP_GATT_IF_NONE || gatts_if == GL_PROFILE.if_ {
//         gatt_profile_callback(event, gatts_if, param);
//     }
// }

// static DEVICE_NAME: &[u8] = b"D21 Door Thingie\0";

// static mut ADV_CONFIG_DONE: u8 = 0;
// const ADV_CONFIG_FLAG: u8 = 1 << 0;
// const SCAN_RSP_CONFIG_FLAG: u8 = 1 << 1;

// static ADV_SERVICE_UUID: [u8; 32] = [0x69; 32];
// const GATTS_NUM_HANDLES: u16 = 4;

// static mut ADV_DATA: esp_idf_sys::esp_ble_adv_data_t = esp_idf_sys::esp_ble_adv_data_t {
//     set_scan_rsp: false,
//     include_name: true,
//     include_txpower: false,
//     min_interval: 0x0006,
//     max_interval: 0x0010,
//     appearance: 0x00,
//     manufacturer_len: 0,
//     p_manufacturer_data: ptr::null_mut(),
//     service_data_len: 0,
//     p_service_data: ptr::null_mut(),
//     service_uuid_len: std::mem::size_of::<[u8; 32]>() as u16,
//     p_service_uuid: &ADV_SERVICE_UUID as *const _ as *mut _,
//     flag: (esp_idf_sys::ESP_BLE_ADV_FLAG_GEN_DISC | esp_idf_sys::ESP_BLE_ADV_FLAG_BREDR_NOT_SPT)
//         as u8,
// };

// static mut SCAN_RSP_DATA: esp_idf_sys::esp_ble_adv_data_t = esp_idf_sys::esp_ble_adv_data_t {
//     set_scan_rsp: true,
//     include_name: true,
//     include_txpower: true,
//     min_interval: 0x0,
//     max_interval: 0x0,
//     appearance: 0x00,
//     manufacturer_len: 0,
//     p_manufacturer_data: ptr::null_mut(),
//     service_data_len: 0,
//     p_service_data: ptr::null_mut(),
//     service_uuid_len: std::mem::size_of::<[u8; 32]>() as u16,
//     p_service_uuid: &ADV_SERVICE_UUID as *const _ as *mut _,
//     flag: (esp_idf_sys::ESP_BLE_ADV_FLAG_GEN_DISC | esp_idf_sys::ESP_BLE_ADV_FLAG_BREDR_NOT_SPT)
//         as u8,
// };

// static mut GATT_PROPERTY: esp_idf_sys::esp_gatt_char_prop_t = 0;

// const GATTS_DEMO_CHAR_VAL_LEN_MAX: u16 = 0x40;
// static mut CHAR1_STR: [u8; 3] = [0x11, 0x22, 0x33];

// static mut GATTS_CHAR1_VAL: esp_idf_sys::esp_attr_value_t = esp_idf_sys::esp_attr_value_t {
//     attr_max_len: GATTS_DEMO_CHAR_VAL_LEN_MAX,
//     attr_len: std::mem::size_of::<[u8; 3]>() as u16,
//     attr_value: unsafe { &mut CHAR1_STR as *const _ as *mut _ },
// };

// unsafe fn gatt_profile_callback(
//     event: esp_idf_sys::esp_gatts_cb_event_t,
//     gatts_if: esp_idf_sys::esp_gatt_if_t,
//     param: *mut esp_idf_sys::esp_ble_gatts_cb_param_t,
// ) {
//     let param_ = *param;

//     match event {
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_REG_EVT => {
//             GL_PROFILE.service_id.is_primary = true;
//             GL_PROFILE.service_id.id.inst_id = 0x00;
//             GL_PROFILE.service_id.id.uuid.len = esp_idf_sys::ESP_UUID_LEN_16 as u16;
//             GL_PROFILE.service_id.id.uuid.uuid.uuid16 = 0x6969;

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gap_set_device_name(
//                 DEVICE_NAME.as_ptr() as *const i8
//             )) {
//                 error!(?err, "Error setting device name");
//                 return;
//             }

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gap_config_adv_data(&mut ADV_DATA)) {
//                 error!(?err, "Error setting gap adv config");
//                 return;
//             }

//             ADV_CONFIG_DONE |= ADV_CONFIG_FLAG;

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gap_config_adv_data(&mut SCAN_RSP_DATA)) {
//                 error!(?err, "Error setting gap scan rsp config");
//                 return;
//             }

//             ADV_CONFIG_DONE |= SCAN_RSP_CONFIG_FLAG;

//             esp_idf_sys::esp_ble_gatts_create_service(
//                 gatts_if,
//                 &mut GL_PROFILE.service_id,
//                 GATTS_NUM_HANDLES,
//             );
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_READ_EVT => {
//             info!(read = ?param_.read, "BLE GATTS read_evt");
//             let mut rsp = esp_idf_sys::esp_gatt_rsp_t::default();
//             rsp.attr_value.handle = param_.read.handle;
//             rsp.attr_value.len = 4;
//             rsp.attr_value.value[0] = 0x69;
//             rsp.attr_value.value[1] = 0x69;
//             rsp.attr_value.value[2] = 0x69;
//             rsp.attr_value.value[3] = 0x69;

//             esp_idf_sys::esp_ble_gatts_send_response(
//                 gatts_if,
//                 param_.read.conn_id,
//                 param_.read.trans_id,
//                 esp_gatt_status_t_ESP_GATT_OK,
//                 &mut rsp,
//             );
//         }
//         // esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_REG_EVT => {}
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_MTU_EVT => {
//             info!("Setting MTU to {}", param_.mtu.mtu);
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_UNREG_EVT => {}
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_CREATE_EVT => {
//             info!(status = ?param_.create.status,
//                   service_handle = ?param_.create.service_handle,
//                   "Creating GATTS service");
//             GL_PROFILE.service_handle = param_.create.service_handle;
//             GL_PROFILE.char_uuid.len = esp_idf_sys::ESP_UUID_LEN_16 as u16;
//             GL_PROFILE.char_uuid.uuid.uuid16 = 0x0420;

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gatts_start_service(
//                 GL_PROFILE.service_handle
//             )) {
//                 error!(?err, "Error starting GATTS service");
//                 return;
//             }

//             GATT_PROPERTY = (esp_idf_sys::ESP_GATT_CHAR_PROP_BIT_READ
//                 | esp_idf_sys::ESP_GATT_CHAR_PROP_BIT_WRITE
//                 | esp_idf_sys::ESP_GATT_CHAR_PROP_BIT_NOTIFY) as u8;

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gatts_add_char(
//                 GL_PROFILE.service_handle,
//                 &mut GL_PROFILE.char_uuid,
//                 (ESP_GATT_PERM_READ | ESP_GATT_PERM_WRITE) as u16,
//                 GATT_PROPERTY,
//                 &mut GATTS_CHAR1_VAL,
//                 std::ptr::null_mut()
//             )) {
//                 error!(?err, "Error adding char to GATTS");
//                 return;
//             }
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_ADD_INCL_SRVC_EVT => {}
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_ADD_CHAR_EVT => {
//             info!(status = ?param_.add_char.status,
//                   attr_handle = ?param_.add_char.attr_handle,
//                   service_handle = ?param_.add_char.service_handle,
//                   "Adding char to service");
//             GL_PROFILE.char_handle = param_.add_char.attr_handle;
//             GL_PROFILE.descr_uuid.len = esp_idf_sys::ESP_UUID_LEN_16 as u16;
//             GL_PROFILE.descr_uuid.uuid.uuid16 = 0x4200;

//             let mut length: u16 = 0;
//             let mut prf_char: *const u8 = std::ptr::null();

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gatts_get_attr_value(
//                 param_.add_char.attr_handle,
//                 &mut length,
//                 &mut prf_char
//             )) {
//                 error!(?err, "Error getting attribute value");
//             }

//             info!(length, "GATTS Char length");
//             let s = std::slice::from_raw_parts(prf_char, length as usize);
//             info!("GATTS Char = {:?}", s);

//             if let Err(err) = esp!(esp_idf_sys::esp_ble_gatts_add_char_descr(
//                 GL_PROFILE.service_handle,
//                 &mut GL_PROFILE.descr_uuid,
//                 (ESP_GATT_PERM_READ | ESP_GATT_PERM_WRITE) as u16,
//                 std::ptr::null_mut(),
//                 std::ptr::null_mut()
//             )) {
//                 error!(?err, "Error adding char descr to GATTS");
//                 return;
//             }
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_ADD_CHAR_DESCR_EVT => {
//             GL_PROFILE.descr_handle = param_.add_char_descr.attr_handle;
//             info!(status = ?param_.add_char_descr.status,
//                   attr_handle = ?param_.add_char_descr.attr_handle,
//                   service_handle = ?param_.add_char_descr.service_handle,
//                   "Added char descr");
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_DELETE_EVT => {}
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_START_EVT => {
//             info!(status = ?param_.start.status,
//                   service_handle = ?param_.start.service_handle,
//                   "Started GATTS");
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_STOP_EVT => {}
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_CONNECT_EVT => {
//             let mut conn_params = esp_idf_sys::esp_ble_conn_update_params_t::default();
//             conn_params.bda = param_.connect.remote_bda;
//             conn_params.latency = 0;
//             conn_params.max_int = 0x20;
//             conn_params.min_int = 0x10;
//             conn_params.timeout = 400;
//             info!(connect = ?param_.connect, "Got GATT Connection");
//             GL_PROFILE.conn_id = param_.connect.conn_id;
//             esp_idf_sys::esp_ble_gap_update_conn_params(&mut conn_params);
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_DISCONNECT_EVT => {
//             info!(disconnect = ?param_.disconnect, "GATTS Disconnect");
//             esp_idf_sys::esp_ble_gap_start_advertising(&mut ADV_PARAMS);
//         }
//         esp_idf_sys::esp_gatts_cb_event_t_ESP_GATTS_CONF_EVT => {
//             info!(conf = ?param_.conf, "ESP Conf event");
//         }
//         _ => {
//             info!("Received unhandled GATT event: {}", event)
//         }
//     }
// }

// static mut ADV_PARAMS: esp_idf_sys::esp_ble_adv_params_t = esp_idf_sys::esp_ble_adv_params_t {
//     adv_int_min: 0x20,
//     adv_int_max: 0x40,
//     adv_type: esp_idf_sys::esp_ble_adv_type_t_ADV_TYPE_IND,
//     own_addr_type: esp_idf_sys::esp_ble_addr_type_t_BLE_ADDR_TYPE_PUBLIC,
//     peer_addr: [0; 6],
//     peer_addr_type: 0,
//     channel_map: esp_idf_sys::esp_ble_adv_channel_t_ADV_CHNL_ALL,
//     adv_filter_policy: esp_idf_sys::esp_ble_adv_filter_t_ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY,
// };

// unsafe extern "C" fn ble_gap_callback(
//     event: esp_idf_sys::esp_gap_ble_cb_event_t,
//     param: *mut esp_idf_sys::esp_ble_gap_cb_param_t,
// ) {
//     let param = *param;

//     match event {
//         esp_idf_sys::esp_gap_ble_cb_event_t_ESP_GAP_BLE_ADV_DATA_SET_COMPLETE_EVT => {
//             ADV_CONFIG_DONE &= !ADV_CONFIG_FLAG;
//             if ADV_CONFIG_DONE == 0 {
//                 if let Err(e) = esp!(esp_idf_sys::esp_ble_gap_start_advertising(&mut ADV_PARAMS)) {
//                     error!("BLE Advertising start err: {:?}", e);
//                     return;
//                 }
//             }
//         }
//         esp_idf_sys::esp_gap_ble_cb_event_t_ESP_GAP_BLE_SCAN_RSP_DATA_SET_COMPLETE_EVT => {
//             ADV_CONFIG_DONE &= !SCAN_RSP_CONFIG_FLAG;
//             if ADV_CONFIG_DONE == 0 {
//                 if let Err(e) = esp!(esp_idf_sys::esp_ble_gap_start_advertising(&mut ADV_PARAMS)) {
//                     error!("BLE Advertising start err: {:?}", e);
//                     return;
//                 }
//             }
//         }
//         esp_idf_sys::esp_gap_ble_cb_event_t_ESP_GAP_BLE_ADV_START_COMPLETE_EVT => {
//             if param.adv_start_cmpl.status != esp_idf_sys::esp_bt_status_t_ESP_BT_STATUS_SUCCESS {
//                 error!("BLE advertising start failed");
//                 return;
//             }
//         }
//         esp_idf_sys::esp_gap_ble_cb_event_t_ESP_GAP_BLE_ADV_STOP_COMPLETE_EVT => {
//             if param.adv_stop_cmpl.status != esp_idf_sys::esp_bt_status_t_ESP_BT_STATUS_SUCCESS {
//                 error!("BLE advertising stop failed");
//                 return;
//             } else {
//                 info!("BLE advertising stop succeeded");
//             }
//         }
//         esp_idf_sys::esp_gap_ble_cb_event_t_ESP_GAP_BLE_UPDATE_CONN_PARAMS_EVT => {
//             info!(status = ?param.update_conn_params, "BLE connection status update");
//         }
//         _ => info!("Received unhandled GAP event: {}", event),
//     }
// }
