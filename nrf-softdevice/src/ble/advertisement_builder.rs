#[cfg(feature = "defmt")]
use defmt::Format;

const STD_LEN: usize = 31;
const EXT_LEN: usize = 254;

#[cfg_attr(feature = "defmt", derive(Format))]
pub enum Error {
    Oversize { expected: usize },
}

#[allow(non_camel_case_types)]
#[repr(u8)]
pub enum ADType {
    Flags = 0x01,
    Incomplete16ServiceList,
    Complete16ServiceList,
    Incomplete32ServiceList,
    Complete32ServiceList,
    Incomplete128ServiceList,
    Complete128ServiceList,
    ShortName,
    FullName,
    TXPowerLevel,
    PeripheralConnectionIntervalRange = 0x12,
    ServiceSolicitation16 = 0x14,
    ServiceSolicitation128,
    ServiceSolicitation32 = 0x1f,
    ServiceData16 = 0x16,
    ServiceData32 = 0x20,
    ServiceData128,
    Appearance = 0x19,
    PublicTargetAddress = 0x17,
    RandomTargetAddress,
    AdvertisingInterval = 0x1a,
    URI = 0x24,
    LE_SupportedFeatures = 0x27,
    ManufacturerSpecificData = 0xff,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Flag {
    LimitedDiscovery = 0b1,
    GeneralDiscovery = 0b10,
    LE_Only = 0b100,

    // i don't understand these but in case people want them
    Bit3 = 0b1000,
    Bit4 = 0b10000,
    // the rest are "reserved for future use"
}

pub trait Service {
    const SIZE: usize;

    fn render<const N: usize>(self, adv: &mut AdvertisementData<N>);
}

pub trait ServiceList<S: Service, const N: usize> {
    const AD: ADType;

    fn list(self) -> [S; N];
}

#[derive(Clone)]
#[repr(u16)]
pub enum BasicService {
    GenericAccess = 0x1800,
    GenericAttribute,
    ImmediateAlert,
    LinkLoss,
    TxPower,
    CurrentTime,
    ReferenceTimeUpdate,
    NextDSTChange,
    Glucose,
    HealthThermometer,
    DeviceInformation,
    HeartRate = 0x180d,
    PhoneAlertStatus,
    Battery,
    BloodPressure,
    AlertNotification,
    HumanInterfaceDevice,
    ScanParameters,
    RunnnigSpeedAndCadence,
    AutomationIO,
    CyclingSpeedAndCadence,
    CyclingPower = 0x1818,
    LocationAndNavigation,
    EnvironmentalSensing,
    BodyComposition,
    UserData,
    WeightScale,
    BondManagement,
    ContinousGlucoseMonitoring,
    InternetProtocolSupport,
    IndoorPositioning,
    PulseOximeter,
    HTTPProxy,
    TransportDiscovery,
    ObjectTransfer,
    FitnessMachine,
    MeshProvisioning,
    MeshProxy,
    ReconnectionConfiguration,
    InsulinDelivery = 0x183a,
    BinarySensor,
    EmergencyConfiguration,
    AuthorizationControl,
    PhysicalActivityMonitor,
    ElapsedTime,
    GenericHealthSensor,
    AudioInputControl = 0x1843,
    VolumeControl,
    VolumeOffsetControl,
    CoordinatedSetIdentification,
    DeviceTime,
    MediaControl,
    GenericMediaControl, // why??
    ConstantToneExtension,
    TelephoneBearer,
    GenericTelephoneBearer,
    MicrophoneControl,
    AudioStreamControl,
    BroadcastAudioScan,
    PublishedAudioScan,
    BasicAudioCapabilities,
    BroadcastAudioAnnouncement,
    CommonAudio,
    HearingAccess,
    TelephonyAndMediaAudio,
    PublicBroadcastAnnouncement,
    ElectronicShelfLabel,
    GamingAudio,
    MeshProxySolicitation,
}

impl Service for BasicService {
    const SIZE: usize = 2;

    fn render<const N: usize>(self, adv: &mut AdvertisementData<N>) {
        let data = (self as u16).swap_bytes().to_be_bytes();
        adv.write(&data);
    }
}

pub struct CustomService(pub [u8; 16]);

impl Service for CustomService {
    const SIZE: usize = 16;

    fn render<const N: usize>(mut self, adv: &mut AdvertisementData<N>) {
        self.0.reverse();
        adv.write(&self.0);
    }
}

pub struct Incomplete16<const N: usize>(pub [BasicService; N]);
pub struct Complete16<const N: usize>(pub [BasicService; N]);
pub struct Incomplete128<const N: usize>(pub [CustomService; N]);
pub struct Complete128<const N: usize>(pub [CustomService; N]);

macro_rules! impl_service_list {
    ($LIST:ident, $SERVICE:ident, $AD:ident) => {
        impl<const N: usize> ServiceList<$SERVICE, N> for $LIST<N> {
            const AD: ADType = ADType::$AD;

            fn list(self) -> [$SERVICE; N] {
                self.0
            }
        }
    };
}

impl_service_list!(Incomplete16, BasicService, Incomplete16ServiceList);
impl_service_list!(Complete16, BasicService, Complete16ServiceList);
impl_service_list!(Incomplete128, CustomService, Incomplete128ServiceList);
impl_service_list!(Complete128, CustomService, Complete128ServiceList);

pub trait Name {
    const AD: ADType;

    fn inner(&self) -> &str;
}

pub struct ShortName<'a>(pub &'a str);
pub struct FullName<'a>(pub &'a str);

macro_rules! impl_name {
    ($NAME:ident, $AD:ident) => {
        impl<'a> Name for $NAME<'a> {
            const AD: ADType = ADType::$AD;

            fn inner(&self) -> &str {
                self.0
            }
        }
    };
}

impl_name!(ShortName, ShortName);
impl_name!(FullName, FullName);

pub struct AdvertisementData<const N: usize> {
    buf: [u8; N],
    ptr: usize,
}

impl<const K: usize> AdvertisementData<K> {
    pub fn new() -> Self {
        Self { buf: [0; K], ptr: 0 }
    }

    fn write(&mut self, data: &[u8]) {
        let end = self.ptr + data.len();

        if end <= K {
            self.buf[self.ptr..end].copy_from_slice(data);
        }

        self.ptr += data.len();
    }

    /// Write raw bytes to the advertisement data.
    ///
    /// *Note: The length is automatically computed and prepended.*
    pub fn raw(mut self, ad: ADType, data: &[u8]) -> Self {
        self.write(&[data.len() as u8 + 1, ad as u8]);
        self.write(data);

        self
    }

    /// View the resulting advertisement data in the form of a byte slice.
    pub fn as_slice(&self) -> Result<&[u8], Error> {
        (self.ptr <= K)
            .then(|| &self.buf[..self.ptr])
            .ok_or(Error::Oversize { expected: self.ptr })
    }

    /// Add flags to the advertisement data.
    pub fn flags<const N: usize>(self, flags: [Flag; N]) -> Self {
        let result = flags.iter().fold(0, |partial, &flag| partial + flag as u8);

        self.raw(ADType::Flags, &[result])
    }

    /// Add a list of services to the advertisement data.
    pub fn services<L: ServiceList<S, N>, S: Service, const N: usize>(mut self, services: L) -> Self {
        self.write(&[(N * S::SIZE) as u8 + 1, L::AD as u8]);

        for service in services.list() {
            service.render(&mut self);
        }

        self
    }

    /// Add a name to the advertisement data.
    pub fn name<N: Name>(self, name: N) -> Self {
        self.raw(N::AD, name.inner().as_bytes())
    }

    /// If the full name fits within the remaining space, it is used. Otherwise the short name is used.
    ///
    /// *Note: This modifier should be placed last.*
    pub fn adapt_name(self, full: FullName, short: ShortName) -> Self {
        let full_len = full.inner().len();

        if self.ptr + full_len <= K {
            self.name(full)
        } else {
            self.name(short)
        }
    }
}

pub type StandardAdvertisementData = AdvertisementData<STD_LEN>;
pub type ExtendedAdvertisementData = AdvertisementData<EXT_LEN>;