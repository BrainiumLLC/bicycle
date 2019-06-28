fn require_send<T: Send>() {}
fn require_sync<T: Sync>() {}

#[test]
fn bicycle_send() {
    require_send::<bicycle::Bicycle>();
}

#[test]
fn bicycle_sync() {
    require_sync::<bicycle::Bicycle>();
}
