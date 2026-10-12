pub const USHRT_MAX: u16 = 65535;

pub const CMD_DB_RRQ: u16 = 7;
pub const CMD_USER_WRQ: u16 = 8;
pub const CMD_USERTEMP_RRQ: u16 = 9;
pub const CMD_USERTEMP_WRQ: u16 = 10;
pub const CMD_OPTIONS_RRQ: u16 = 11;
pub const CMD_OPTIONS_WRQ: u16 = 12;
pub const CMD_ATTLOG_RRQ: u16 = 13;
pub const CMD_CLEAR_DATA: u16 = 14;
pub const CMD_CLEAR_ATTLOG: u16 = 15;
pub const CMD_DELETE_USER: u16 = 18;
pub const CMD_DELETE_USERTEMP: u16 = 19;
pub const CMD_CLEAR_ADMIN: u16 = 20;
pub const CMD_USERGRP_RRQ: u16 = 21;
pub const CMD_USERGRP_WRQ: u16 = 22;
pub const CMD_USERTZ_RRQ: u16 = 23;
pub const CMD_USERTZ_WRQ: u16 = 24;
pub const CMD_GRPTZ_RRQ: u16 = 25;
pub const CMD_GRPTZ_WRQ: u16 = 26;
pub const CMD_TZ_RRQ: u16 = 27;
pub const CMD_TZ_WRQ: u16 = 28;
pub const CMD_ULG_RRQ: u16 = 29;
pub const CMD_ULG_WRQ: u16 = 30;
pub const CMD_UNLOCK: u16 = 31;
pub const CMD_CLEAR_ACC: u16 = 32;
pub const CMD_CLEAR_OPLOG: u16 = 33;
pub const CMD_OPLOG_RRQ: u16 = 34;
pub const CMD_GET_FREE_SIZES: u16 = 50;
pub const CMD_ENABLE_CLOCK: u16 = 57;
pub const CMD_STARTVERIFY: u16 = 60;
pub const CMD_STARTENROLL: u16 = 61;
pub const CMD_CANCELCAPTURE: u16 = 62;
pub const CMD_STATE_RRQ: u16 = 64;
pub const CMD_WRITE_LCD: u16 = 66;
pub const CMD_CLEAR_LCD: u16 = 67;
pub const CMD_GET_PINWIDTH: u16 = 69;
pub const CMD_SMS_WRQ: u16 = 70;
pub const CMD_SMS_RRQ: u16 = 71;
pub const CMD_DELETE_SMS: u16 = 72;
pub const CMD_UDATA_WRQ: u16 = 73;
pub const CMD_DELETE_UDATA: u16 = 74;
pub const CMD_DOORSTATE_RRQ: u16 = 75;
pub const CMD_WRITE_MIFARE: u16 = 76;
pub const CMD_EMPTY_MIFARE: u16 = 78;
pub const _CMD_GET_USERTEMP: u16 = 88;
pub const _CMD_SAVE_USERTEMPS: u16 = 110;
pub const _CMD_DEL_USER_TEMP: u16 = 134;

pub const CMD_GET_TIME: u16 = 201;
pub const CMD_SET_TIME: u16 = 202;
pub const CMD_REG_EVENT: u16 = 500;

pub const CMD_CONNECT: u16 = 1000;
pub const CMD_EXIT: u16 = 1001;
pub const CMD_ENABLEDEVICE: u16 = 1002;
pub const CMD_DISABLEDEVICE: u16 = 1003;
pub const CMD_RESTART: u16 = 1004;
pub const CMD_POWEROFF: u16 = 1005;
pub const CMD_SLEEP: u16 = 1006;
pub const CMD_RESUME: u16 = 1007;
pub const CMD_CAPTUREFINGER: u16 = 1009;
pub const CMD_TEST_TEMP: u16 = 1011;
pub const CMD_CAPTUREIMAGE: u16 = 1012;
pub const CMD_REFRESHDATA: u16 = 1013;
pub const CMD_REFRESHOPTION: u16 = 1014;
pub const CMD_TESTVOICE: u16 = 1017;
pub const CMD_GET_VERSION: u16 = 1100;
pub const CMD_CHANGE_SPEED: u16 = 1101;
pub const CMD_AUTH: u16 = 1102;
pub const CMD_PREPARE_DATA: u16 = 1500;
pub const CMD_DATA: u16 = 1501;
pub const CMD_FREE_DATA: u16 = 1502;
pub const _CMD_PREPARE_BUFFER: u16 = 1503;
pub const _CMD_READ_BUFFER: u16 = 1504;

pub const CMD_ACK_OK: u16 = 2000;
pub const CMD_ACK_ERROR: u16 = 2001;
pub const CMD_ACK_DATA: u16 = 2002;
pub const CMD_ACK_RETRY: u16 = 2003;
pub const CMD_ACK_REPEAT: u16 = 2004;
pub const CMD_ACK_UNAUTH: u16 = 2005;

pub const CMD_ACK_UNKNOWN: u16 = 0xffff;
pub const CMD_ACK_ERROR_CMD: u16 = 0xfffd;
pub const CMD_ACK_ERROR_INIT: u16 = 0xfffc;
pub const CMD_ACK_ERROR_DATA: u16 = 0xfffb;

pub const EF_ATTLOG: u32 = 1;
pub const EF_FINGER: u32 = 1 << 1;
pub const EF_ENROLLUSER: u32 = 1 << 2;
pub const EF_ENROLLFINGER: u32 = 1 << 3;
pub const EF_BUTTON: u32 = 1 << 4;
pub const EF_UNLOCK: u32 = 1 << 5;
pub const EF_VERIFY: u32 = 1 << 7;
pub const EF_FPFTR: u32 = 1 << 8;
pub const EF_ALARM: u32 = 1 << 9;

pub const USER_DEFAULT: u8 = 0;
pub const USER_ENROLLER: u8 = 2;
pub const USER_MANAGER: u8 = 6;
pub const USER_ADMIN: u8 = 14;

pub const FCT_ATTLOG: u8 = 1;
pub const FCT_WORKCODE: u8 = 8;
pub const FCT_FINGERTMP: u8 = 2;
pub const FCT_OPLOG: u8 = 4;
pub const FCT_USER: u8 = 5;
pub const FCT_SMS: u8 = 6;
pub const FCT_UDATA: u8 = 7;

pub const MACHINE_PREPARE_DATA_1: u16 = 20560; // 0x5050
pub const MACHINE_PREPARE_DATA_2: u16 = 32130; // 0x7282

pub const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit for safety
pub const MAX_DISCARDED_PACKETS: usize = 100;
pub const TCP_MAX_CHUNK: usize = 0xFFC0;
pub const UDP_MAX_CHUNK: usize = 16 * 1024;

// User packet sizes
pub const USER_PACKET_SIZE_SMALL: usize = 28;
pub const USER_PACKET_SIZE_LARGE: usize = 72;

// Attendance record sizes
pub const ATT_RECORD_SIZE_8: usize = 8;
pub const ATT_RECORD_SIZE_16: usize = 16;
pub const ATT_RECORD_SIZE_40: usize = 40;

// Event data lengths
pub const EVENT_DATA_LEN_10: usize = 10;
pub const EVENT_DATA_LEN_12: usize = 12;
pub const EVENT_DATA_LEN_32: usize = 32;

// ── read_sizes() payload field offsets ────────────────────────────────────
//
// V2 firmware format (payload >= 80 bytes). Each field is a 4-byte LE i32.
pub const SIZES_V2_USERS: usize = 16;
pub const SIZES_V2_FINGERS: usize = 24;
pub const SIZES_V2_RECORDS: usize = 32;
pub const SIZES_V2_CARDS: usize = 48;
pub const SIZES_V2_FINGERS_CAP: usize = 56;
pub const SIZES_V2_USERS_CAP: usize = 60;
pub const SIZES_V2_REC_CAP: usize = 64;
pub const SIZES_V2_FACES: usize = 80;
pub const SIZES_V2_FACES_CAP: usize = 88;

/// Minimum payload length for V2 extended format (includes faces).
pub const SIZES_V2_EXT_MIN: usize = 92;

/// Minimum payload length for V2 base format (users through rec_cap).
pub const SIZES_V2_MIN: usize = 80;

// V1 firmware format (28 <= payload < 80 bytes). Each field is a 4-byte LE i32.
pub const SIZES_V1_USERS: usize = 0;
pub const SIZES_V1_FINGERS: usize = 4;
pub const SIZES_V1_RECORDS: usize = 8;
pub const SIZES_V1_USERS_CAP: usize = 12;
pub const SIZES_V1_FINGERS_CAP: usize = 16;
pub const SIZES_V1_REC_CAP: usize = 20;

/// Minimum payload length for V1 format.
pub const SIZES_V1_MIN: usize = 28;

/// Minimum payload size for ACK_OK data fallback in read_sizes().
pub const SIZES_ACK_FALLBACK_MIN: usize = 16;

/// Size of each device info field (i32 = 4 bytes).
pub const SIZES_FIELD_LEN: usize = 4;
