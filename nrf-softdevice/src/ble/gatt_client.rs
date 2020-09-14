//! Generic Attribute client. GATT clients consume functionality offered by GATT servers.

use heapless::consts::*;
use heapless::Vec;
use num_enum::{FromPrimitive, IntoPrimitive};

use crate::ble::*;
use crate::raw;
use crate::util::*;
use crate::RawError;

/// Discovered characteristic
pub struct Characteristic {
    pub uuid: Option<Uuid>,
    pub handle_decl: u16,
    pub handle_value: u16,
    pub props: raw::ble_gatt_char_props_t,
    pub has_ext_props: bool,
}

/// Discovered descriptor
pub struct Descriptor {
    pub uuid: Option<Uuid>,
    pub handle: u16,
}

/// Trait for implementing GATT clients.
pub trait Client {
    /// Get the UUID of the GATT service. This is used by [`discover`] to search for the
    /// service in the GATT server.
    fn uuid() -> Uuid;

    /// Create a new instance in a "not-yet-discovered" state.
    fn new_undiscovered(conn: Connection) -> Self;

    /// Called by [`discover`] for every discovered characteristic. Implementations must
    /// check if they're interested in the UUID of the characteristic, and save their
    /// handles if needed.
    fn discovered_characteristic(
        &mut self,
        characteristic: &Characteristic,
        descriptors: &[Descriptor],
    );

    /// Called by [`discover`] at the end of the discovery procedure. Implementations must check
    /// that all required characteristics have been discovered, and return [`DiscoverError::ServiceIncomplete`]
    /// otherwise.
    ///
    /// If no error is returned, this instance is considered ready to use and is returned to
    /// the caller of [`discover`]
    fn discovery_complete(&mut self) -> Result<(), DiscoverError>;
}

#[rustfmt::skip]
#[repr(u32)]
#[derive(defmt::Format, IntoPrimitive, FromPrimitive)]
pub enum GattError {
    // This is not really an error, but IMO it's better to add it
    // anyway, just in case someone mistakenly converts BLE_GATT_STATUS_SUCCESS into GattError.
    // if they see "Success" they'll easily realize their mistake, if they see "Unknown" it'd be confusing.
    Success = raw::BLE_GATT_STATUS_SUCCESS,

    #[num_enum(default)]
    Unknown = raw::BLE_GATT_STATUS_UNKNOWN,

    AtterrInvalid = raw::BLE_GATT_STATUS_ATTERR_INVALID,
    AtterrInvalidHandle = raw::BLE_GATT_STATUS_ATTERR_INVALID_HANDLE,
    AtterrReadNotPermitted = raw::BLE_GATT_STATUS_ATTERR_READ_NOT_PERMITTED,
    AtterrWriteNotPermitted = raw::BLE_GATT_STATUS_ATTERR_WRITE_NOT_PERMITTED,
    AtterrInvalidPdu = raw::BLE_GATT_STATUS_ATTERR_INVALID_PDU,
    AtterrInsufAuthentication = raw::BLE_GATT_STATUS_ATTERR_INSUF_AUTHENTICATION,
    AtterrRequestNotSupported = raw::BLE_GATT_STATUS_ATTERR_REQUEST_NOT_SUPPORTED,
    AtterrInvalidOffset = raw::BLE_GATT_STATUS_ATTERR_INVALID_OFFSET,
    AtterrInsufAuthorization = raw::BLE_GATT_STATUS_ATTERR_INSUF_AUTHORIZATION,
    AtterrPrepareQueueFull = raw::BLE_GATT_STATUS_ATTERR_PREPARE_QUEUE_FULL,
    AtterrAttributeNotFound = raw::BLE_GATT_STATUS_ATTERR_ATTRIBUTE_NOT_FOUND,
    AtterrAttributeNotLong = raw::BLE_GATT_STATUS_ATTERR_ATTRIBUTE_NOT_LONG,
    AtterrInsufEncKeySize = raw::BLE_GATT_STATUS_ATTERR_INSUF_ENC_KEY_SIZE,
    AtterrInvalidAttValLength = raw::BLE_GATT_STATUS_ATTERR_INVALID_ATT_VAL_LENGTH,
    AtterrUnlikelyError = raw::BLE_GATT_STATUS_ATTERR_UNLIKELY_ERROR,
    AtterrInsufEncryption = raw::BLE_GATT_STATUS_ATTERR_INSUF_ENCRYPTION,
    AtterrUnsupportedGroupType = raw::BLE_GATT_STATUS_ATTERR_UNSUPPORTED_GROUP_TYPE,
    AtterrInsufResources = raw::BLE_GATT_STATUS_ATTERR_INSUF_RESOURCES,
    AtterrCpsWriteReqRejected = raw::BLE_GATT_STATUS_ATTERR_CPS_WRITE_REQ_REJECTED,
    AtterrCpsCccdConfigError = raw::BLE_GATT_STATUS_ATTERR_CPS_CCCD_CONFIG_ERROR,
    AtterrCpsProcAlrInProg = raw::BLE_GATT_STATUS_ATTERR_CPS_PROC_ALR_IN_PROG,
    AtterrCpsOutOfRange = raw::BLE_GATT_STATUS_ATTERR_CPS_OUT_OF_RANGE,
}

/// Error type for [`discover`]
#[derive(defmt::Format)]
pub enum DiscoverError {
    /// Connection is disconnected.
    Disconnected,
    /// No service with the given UUID found in the server.
    ServiceNotFound,
    /// Service with the given UUID found, but it's missing some required characteristics.
    ServiceIncomplete,
    Gatt(GattError),
    Raw(RawError),
}

impl From<DisconnectedError> for DiscoverError {
    fn from(_: DisconnectedError) -> Self {
        DiscoverError::Disconnected
    }
}

impl From<GattError> for DiscoverError {
    fn from(err: GattError) -> Self {
        DiscoverError::Gatt(err)
    }
}

impl From<RawError> for DiscoverError {
    fn from(err: RawError) -> Self {
        DiscoverError::Raw(err)
    }
}

type DiscCharsMax = U6;
type DiscDescsMax = U6;

pub(crate) enum PortalMessage {
    DiscoverService(Result<raw::ble_gattc_service_t, DiscoverError>),
    DiscoverCharacteristics(Result<Vec<raw::ble_gattc_char_t, DiscCharsMax>, DiscoverError>),
    DiscoverDescriptors(Result<Vec<raw::ble_gattc_desc_t, DiscDescsMax>, DiscoverError>),
    Disconnected,
}

pub(crate) async fn discover_service(
    conn: &Connection,
    uuid: Uuid,
) -> Result<raw::ble_gattc_service_t, DiscoverError> {
    let state = conn.state();
    let conn_handle = state.check_connected()?;
    let ret =
        unsafe { raw::sd_ble_gattc_primary_services_discover(conn_handle, 1, uuid.as_raw_ptr()) };
    RawError::convert(ret).dewarn(intern!("sd_ble_gattc_primary_services_discover"))?;

    match state.gattc_portal.wait().await {
        PortalMessage::DiscoverService(r) => r,
        PortalMessage::Disconnected => Err(DiscoverError::Disconnected),
        _ => unreachable!(),
    }
}

pub(crate) unsafe fn on_prim_srvc_disc_rsp(
    ble_evt: *const raw::ble_evt_t,
    gattc_evt: &raw::ble_gattc_evt_t,
) {
    let val = check_status(gattc_evt, || {
        let params = get_union_field(ble_evt, &gattc_evt.params.prim_srvc_disc_rsp);
        let v = get_flexarray(ble_evt, &params.services, params.count as usize);

        match v.len() {
            0 => Err(DiscoverError::ServiceNotFound),
            1 => Ok(v[0]),
            n => {
                warn!(
                    "Found {:u16} services with the same UUID, using the first one",
                    params.count
                );
                Ok(v[0])
            }
        }
    });

    ConnectionState::by_conn_handle(gattc_evt.conn_handle)
        .gattc_portal
        .signal(PortalMessage::DiscoverService(val))
}

// =============================

async fn discover_characteristics(
    conn: &Connection,
    start_handle: u16,
    end_handle: u16,
) -> Result<Vec<raw::ble_gattc_char_t, DiscCharsMax>, DiscoverError> {
    let state = conn.state();
    let conn_handle = state.check_connected()?;

    let ret = unsafe {
        raw::sd_ble_gattc_characteristics_discover(
            conn_handle,
            &raw::ble_gattc_handle_range_t {
                start_handle,
                end_handle,
            },
        )
    };
    RawError::convert(ret).dewarn(intern!("sd_ble_gattc_characteristics_discover"))?;

    match state.gattc_portal.wait().await {
        PortalMessage::DiscoverCharacteristics(r) => r,
        PortalMessage::Disconnected => Err(DiscoverError::Disconnected),
        _ => unreachable!(),
    }
}

pub(crate) unsafe fn on_char_disc_rsp(
    ble_evt: *const raw::ble_evt_t,
    gattc_evt: &raw::ble_gattc_evt_t,
) {
    let val = check_status(gattc_evt, || {
        let params = get_union_field(ble_evt, &gattc_evt.params.char_disc_rsp);
        let v = get_flexarray(ble_evt, &params.chars, params.count as usize);
        let v = Vec::from_slice(v).unwrap_or_else(|_| {
            depanic!("too many gatt chars, increase DiscCharsMax: {:?}", v.len())
        });
        Ok(v)
    });

    ConnectionState::by_conn_handle(gattc_evt.conn_handle)
        .gattc_portal
        .signal(PortalMessage::DiscoverCharacteristics(val))
}

// =============================

async fn discover_descriptors(
    conn: &Connection,
    start_handle: u16,
    end_handle: u16,
) -> Result<Vec<raw::ble_gattc_desc_t, DiscDescsMax>, DiscoverError> {
    let state = conn.state();
    let conn_handle = state.check_connected()?;

    let ret = unsafe {
        raw::sd_ble_gattc_descriptors_discover(
            conn_handle,
            &raw::ble_gattc_handle_range_t {
                start_handle,
                end_handle,
            },
        )
    };
    RawError::convert(ret).dewarn(intern!("sd_ble_gattc_descriptors_discover"))?;

    match state.gattc_portal.wait().await {
        PortalMessage::DiscoverDescriptors(r) => r,
        PortalMessage::Disconnected => Err(DiscoverError::Disconnected),
        _ => unreachable!(),
    }
}

pub(crate) unsafe fn on_desc_disc_rsp(
    ble_evt: *const raw::ble_evt_t,
    gattc_evt: &raw::ble_gattc_evt_t,
) {
    let val = check_status(gattc_evt, || {
        let params = get_union_field(ble_evt, &gattc_evt.params.desc_disc_rsp);
        let v = get_flexarray(ble_evt, &params.descs, params.count as usize);
        let v = Vec::from_slice(v).unwrap_or_else(|_| {
            depanic!("too many gatt descs, increase DiscDescsMax: {:?}", v.len())
        });
        Ok(v)
    });

    ConnectionState::by_conn_handle(gattc_evt.conn_handle)
        .gattc_portal
        .signal(PortalMessage::DiscoverDescriptors(val))
}

// =============================

async fn discover_inner<T: Client>(
    conn: &Connection,
    client: &mut T,
    svc: &raw::ble_gattc_service_t,
    curr: raw::ble_gattc_char_t,
    next: Option<raw::ble_gattc_char_t>,
) -> Result<(), DiscoverError> {
    let state = conn.state();

    // Calcuate range of possible descriptors
    let start_handle = curr.handle_value + 1;
    let end_handle = next
        .map(|c| c.handle_decl - 1)
        .unwrap_or(svc.handle_range.end_handle);

    let characteristic = Characteristic {
        uuid: Uuid::from_raw(curr.uuid),
        handle_decl: curr.handle_decl,
        handle_value: curr.handle_value,
        has_ext_props: curr.char_ext_props() != 0,
        props: curr.char_props,
    };

    let mut descriptors: Vec<Descriptor, DiscDescsMax> = Vec::new();

    // Only if range is non-empty, discover. (if it's empty there must be no descriptors)
    if start_handle <= end_handle {
        for desc in discover_descriptors(conn, start_handle, end_handle).await? {
            descriptors
                .push(Descriptor {
                    uuid: Uuid::from_raw(desc.uuid),
                    handle: desc.handle,
                })
                .unwrap_or_else(|_| depanic!("no size in descriptors"));
        }
    }

    client.discovered_characteristic(&characteristic, &descriptors[..]);

    Ok(())
}

/// Discover a service in the peer's GATT server and construct a Client instance
/// to use it.
pub async fn discover<T: Client>(conn: &Connection) -> Result<T, DiscoverError> {
    // TODO handle drop. Probably doable gracefully (no DropBomb)

    let svc = match discover_service(conn, T::uuid()).await {
        Err(DiscoverError::Gatt(GattError::AtterrAttributeNotFound)) => {
            Err(DiscoverError::ServiceNotFound)
        }
        x => x,
    }?;

    let mut client = T::new_undiscovered(conn.clone());

    let mut curr_handle = svc.handle_range.start_handle;
    let end_handle = svc.handle_range.end_handle;

    let mut prev_char: Option<raw::ble_gattc_char_t> = None;
    while curr_handle < end_handle {
        let chars = match discover_characteristics(conn, curr_handle, end_handle).await {
            Err(DiscoverError::Gatt(GattError::AtterrAttributeNotFound)) => break,
            x => x,
        }?;
        deassert!(chars.len() != 0);
        for curr in chars {
            if let Some(prev) = prev_char {
                discover_inner(conn, &mut client, &svc, prev, Some(curr)).await?;
            }
            prev_char = Some(curr);
            curr_handle = curr.handle_value + 1;
        }
    }
    if let Some(prev) = prev_char {
        discover_inner(conn, &mut client, &svc, prev, None).await?;
    }

    client.discovery_complete()?;

    Ok(client)
}

fn check_status<T, E: From<GattError>>(
    gattc_evt: &raw::ble_gattc_evt_t,
    f: impl Fn() -> Result<T, E>,
) -> Result<T, E> {
    if gattc_evt.gatt_status as u32 == raw::BLE_GATT_STATUS_SUCCESS {
        f()
    } else {
        Err(GattError::from(gattc_evt.gatt_status as u32).into())
    }
}

pub(crate) unsafe fn on_rel_disc_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_attr_info_disc_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_char_val_by_uuid_read_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_read_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_char_vals_read_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_write_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_hvx(_ble_evt: *const raw::ble_evt_t, _gattc_evt: &raw::ble_gattc_evt_t) {}

pub(crate) unsafe fn on_exchange_mtu_rsp(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_timeout(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}

pub(crate) unsafe fn on_write_cmd_tx_complete(
    _ble_evt: *const raw::ble_evt_t,
    _gattc_evt: &raw::ble_gattc_evt_t,
) {
}
