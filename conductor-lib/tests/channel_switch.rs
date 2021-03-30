use conductor_lib::{channel_switch::ChannelSwitch, proto::DataChannelId};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{
    rustls::{
        self, internal::pemfile, ClientConfig, DangerousClientConfig, NoClientAuth, ServerConfig,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};

#[tokio::test]
async fn channel_switch_handles_one_channel() {
    const EXPECTED_DATA_SIZE: usize = 10;
    const EXPECTED_DATA: [u8; EXPECTED_DATA_SIZE] = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 0];
    const TEST_CHANNEL_ID: DataChannelId = 0;
    const TEST_PORT: u16 = 1613;

    let mut client_switch = ChannelSwitch::default();
    let mut client_io = client_switch
        .create_channel(TEST_CHANNEL_ID)
        .await
        .expect("failed to create channel");

    let mut server_switch = ChannelSwitch::default();
    let mut server_io = server_switch
        .create_channel(TEST_CHANNEL_ID)
        .await
        .expect("failed to create channel");

    tokio::spawn(server_switch.listen(format!("0.0.0.0:{}", TEST_PORT)));
    tokio::spawn(client_switch.connect(format!("127.0.0.1:{}", TEST_PORT)));

    let write_task = tokio::spawn(async move {
        client_io
            .write_all(&EXPECTED_DATA)
            .await
            .map_err(|_| "failed write")?;

        client_io.flush().await.map_err(|_| "failed flush")?;
        Result::<(), &'static str>::Ok(())
    });

    let read_task = tokio::spawn(async move {
        let mut buf = [0u8; 256];

        let read = server_io.read(&mut buf).await.map_err(|_| "failed read")?;
        assert_eq!(read, EXPECTED_DATA_SIZE);

        // all bytes read
        assert_eq!(&buf[..EXPECTED_DATA_SIZE], &EXPECTED_DATA);

        Result::<(), &'static str>::Ok(())
    });

    let (write_result, read_result) = tokio::join!(write_task, read_task);

    match (write_result, read_result) {
        (Ok(Ok(_)), Ok(Ok(_))) => {}
        _ => {
            panic!("transfer failed!")
        }
    }
}

#[tokio::test]
async fn channel_switch_handles_two_channels() {
    const EXPECTED_DATA_CH1_1_SIZE: usize = 10;
    const EXPECTED_DATA_CH1_1: [u8; EXPECTED_DATA_CH1_1_SIZE] = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    const EXPECTED_DATA_CH1_2_SIZE: usize = 10;
    const EXPECTED_DATA_CH1_2: [u8; EXPECTED_DATA_CH1_2_SIZE] = [6u8, 5, 4, 3, 2, 1, 2, 3, 4, 5];

    const EXPECTED_DATA_CH2_1_SIZE: usize = 5;
    const EXPECTED_DATA_CH2_1: [u8; EXPECTED_DATA_CH2_1_SIZE] = [2u8, 1, 3, 4, 8];
    const EXPECTED_DATA_CH2_2_SIZE: usize = 5;
    const EXPECTED_DATA_CH2_2: [u8; EXPECTED_DATA_CH2_2_SIZE] = [7u8, 7, 5, 4, 3];

    const TEST_CH1_ID: DataChannelId = 0;
    const TEST_CH2_ID: DataChannelId = 1;

    const TEST_PORT: u16 = 1614;

    let mut client_switch = ChannelSwitch::default();
    let mut client_ch1_io = client_switch
        .create_channel(TEST_CH1_ID)
        .await
        .expect("failed to create channel");
    let mut client_ch2_io = client_switch
        .create_channel(TEST_CH2_ID)
        .await
        .expect("failed to create channel");

    let mut server_switch = ChannelSwitch::default();
    let mut server_ch1_io = server_switch
        .create_channel(TEST_CH1_ID)
        .await
        .expect("failed to create channel");
    let mut server_ch2_io = server_switch
        .create_channel(TEST_CH2_ID)
        .await
        .expect("failed to create channel");

    tokio::spawn(server_switch.listen(format!("0.0.0.0:{}", TEST_PORT)));
    tokio::spawn(client_switch.connect(format!("127.0.0.1:{}", TEST_PORT)));

    let write_task = tokio::spawn(async move {
        client_ch1_io
            .write_all(&EXPECTED_DATA_CH1_1)
            .await
            .map_err(|_| "failed write")?;
        client_ch1_io.flush().await.map_err(|_| "failed flush")?;
        client_ch2_io
            .write_all(&EXPECTED_DATA_CH2_1)
            .await
            .map_err(|_| "failed write")?;
        client_ch2_io.flush().await.map_err(|_| "failed flush")?;
        client_ch1_io
            .write_all(&EXPECTED_DATA_CH1_2)
            .await
            .map_err(|_| "failed write")?;
        client_ch1_io.flush().await.map_err(|_| "failed flush")?;
        client_ch2_io
            .write_all(&EXPECTED_DATA_CH2_2)
            .await
            .map_err(|_| "failed write")?;
        client_ch2_io.flush().await.map_err(|_| "failed flush")?;

        Result::<(), &'static str>::Ok(())
    });

    let read_ch1_task = tokio::spawn(async move {
        let mut buf = [0u8; 256];

        let mut read = 0;
        while read < EXPECTED_DATA_CH1_1_SIZE + EXPECTED_DATA_CH1_2_SIZE {
            read += server_ch1_io
                .read(&mut buf[read..])
                .await
                .map_err(|_| "failed read")?;
        }

        let mut expected = Vec::new();
        expected.extend_from_slice(&EXPECTED_DATA_CH1_1);
        expected.extend_from_slice(&EXPECTED_DATA_CH1_2);

        // all bytes read
        assert_eq!(
            &buf[..EXPECTED_DATA_CH1_1_SIZE + EXPECTED_DATA_CH1_2_SIZE],
            &expected
        );

        Result::<(), &'static str>::Ok(())
    });

    let read_ch2_task = tokio::spawn(async move {
        let mut buf = [0u8; 256];

        let mut read = 0;
        while read < EXPECTED_DATA_CH2_1_SIZE + EXPECTED_DATA_CH2_2_SIZE {
            read += server_ch2_io
                .read(&mut buf[read..])
                .await
                .map_err(|_| "failed read")?;
        }

        let mut expected = Vec::new();
        expected.extend_from_slice(&EXPECTED_DATA_CH2_1);
        expected.extend_from_slice(&EXPECTED_DATA_CH2_2);

        // all bytes read
        assert_eq!(
            &buf[..EXPECTED_DATA_CH2_1_SIZE + EXPECTED_DATA_CH2_2_SIZE],
            &expected
        );

        Result::<(), &'static str>::Ok(())
    });

    let (write_result, read_ch1_result, read_ch2_result) =
        tokio::join!(write_task, read_ch1_task, read_ch2_task);

    match (write_result, read_ch1_result, read_ch2_result) {
        (Ok(Ok(_)), Ok(Ok(_)), Ok(Ok(_))) => {}
        _ => {
            panic!("transfer failed!")
        }
    }
}

pub struct IngoreCertVerifier;

impl rustls::ServerCertVerifier for IngoreCertVerifier {
    fn verify_server_cert(
        &self,
        _roots: &rustls::RootCertStore,
        _presented_certs: &[rustls::Certificate],
        _dns_name: webpki::DNSNameRef<'_>,
        _ocsp: &[u8],
    ) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
        Ok(rustls::ServerCertVerified::assertion())
    }
}

#[tokio::test]
async fn channel_switch_can_handle_rustls() {
    let cert = include_bytes!("cert/default.crt");
    let mut cert_slice = &cert[..];
    let key = include_bytes!("cert/default.key");
    let mut key_slice = &key[..];

    let cert = pemfile::certs(&mut cert_slice)
        .unwrap()
        .iter()
        .cloned()
        .next()
        .unwrap();

    let private_key = pemfile::rsa_private_keys(&mut key_slice)
        .unwrap()
        .iter()
        .cloned()
        .next()
        .unwrap();

    let mut config = ServerConfig::new(NoClientAuth::new());
    config.set_single_cert(vec![cert], private_key).unwrap();
    let server_config = Arc::new(config);

    let mut config = ClientConfig::new();
    config
        .dangerous()
        .set_certificate_verifier(Arc::new(IngoreCertVerifier));
    let client_config = Arc::new(config);

    let acceptor = TlsAcceptor::from(server_config);
    let connector = TlsConnector::from(client_config);

    const TEST_CHANNEL_ID: DataChannelId = 0;
    const TEST_PORT: u16 = 1615;

    let mut client_switch = ChannelSwitch::default();
    let client_io = client_switch
        .create_channel(TEST_CHANNEL_ID)
        .await
        .expect("failed to create channel");

    let mut server_switch = ChannelSwitch::default();
    let server_io = server_switch
        .create_channel(TEST_CHANNEL_ID)
        .await
        .expect("failed to create channel");

    tokio::spawn(server_switch.listen(format!("0.0.0.0:{}", TEST_PORT)));
    tokio::spawn(client_switch.connect(format!("127.0.0.1:{}", TEST_PORT)));

    let client_task = tokio::spawn(async move {
        let domain = webpki::DNSNameRef::try_from_ascii_str("localhost").unwrap();
        let tls_stream = connector
            .connect(domain, client_io)
            .await
            .expect("failed to perform client handshake");

        Result::<(), &'static str>::Ok(())
    });

    let server_task = tokio::spawn(async move {
        let tls_stream = acceptor
            .accept(server_io)
            .await
            .expect("failed to perform server handshake");

        Result::<(), &'static str>::Ok(())
    });

    let (client_result, server_result) = tokio::join!(client_task, server_task);

    match (client_result, server_result) {
        (Ok(Ok(_)), Ok(Ok(_))) => {}
        _ => {
            panic!("transfer failed!")
        }
    }
}
