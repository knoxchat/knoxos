// ═══════════════════════════════════════════════════════════════════════
// THERMAL MONITORING TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::thermal::{CoolingPolicy, ThermalZone, TripType, ZoneType};

#[test_case]
fn test_thermal_zone_creation() {
    let zone = ThermalZone::new("test-cpu", ZoneType::Estimated);
    assert_eq!(zone.name, "test-cpu");
    assert_eq!(zone.current_temp, 0);
    assert_eq!(zone.sample_count, 0);
}

#[test_case]
fn test_thermal_zone_update() {
    let mut zone = ThermalZone::new("test", ZoneType::Estimated);
    zone.add_trip(TripType::Active, 45, 3);
    zone.add_trip(TripType::Critical, 100, 0);

    zone.update_temp(35000); // 35°C
    assert_eq!(zone.current_temp, 35000);
    assert_eq!(zone.sample_count, 1);
    assert!(!zone.critical);
}

#[test_case]
fn test_thermal_trip_trigger() {
    let mut zone = ThermalZone::new("test", ZoneType::Estimated);
    zone.add_trip(TripType::Active, 45, 3);

    // Below trip point
    zone.update_temp(40000);
    assert!(!zone.trip_points[0].triggered);

    // Above trip point
    zone.update_temp(46000);
    assert!(zone.trip_points[0].triggered);
}

#[test_case]
fn test_thermal_history() {
    let mut zone = ThermalZone::new("test", ZoneType::Estimated);
    for i in 0..10 {
        zone.update_temp(30000 + i * 1000);
    }
    assert_eq!(zone.sample_count, 10);
    assert_eq!(zone.min_temp, 30000);
    assert_eq!(zone.max_temp, 39000);
}

#[test_case]
fn test_thermal_celsius_conversion() {
    let mut zone = ThermalZone::new("test", ZoneType::Estimated);
    zone.update_temp(45500); // 45.5°C
    let (degrees, frac) = zone.temp_celsius();
    assert_eq!(degrees, 45);
    assert_eq!(frac, 500);
}
