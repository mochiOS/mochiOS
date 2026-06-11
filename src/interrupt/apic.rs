pub fn init_local_apic() {
    crate::smp::init_local_apic();
}

pub fn start_ap(apic_id: u32, vector: u8) -> bool {
    crate::smp::start_ap(apic_id, vector)
}
