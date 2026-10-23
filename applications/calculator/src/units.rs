use crate::number::{Number, Result};
#[derive(Clone, Copy)]
pub struct Unit {
    pub name: &'static str,
    pub scale: (i128, i128),
    pub offset: (i128, i128),
}
const fn unit(name: &'static str, n: i128, d: i128) -> Unit {
    Unit {
        name,
        scale: (n, d),
        offset: (0, 1),
    }
}
pub struct Category {
    pub name: &'static str,
    pub units: &'static [Unit],
}
pub static CATEGORIES: &[Category] = &[
    Category {
        name: "Length",
        units: &[
            unit("Metres · m", 1, 1),
            unit("Centimetres · cm", 1, 100),
            unit("Millimetres · mm", 1, 1000),
            unit("Kilometres · km", 1000, 1),
            unit("Inches · in", 127, 5000),
            unit("Feet · ft", 381, 1250),
            unit("Yards · yd", 1143, 1250),
            unit("Miles · mi", 201168, 125),
        ],
    },
    Category {
        name: "Mass",
        units: &[
            unit("Kilograms · kg", 1, 1),
            unit("Grams · g", 1, 1000),
            unit("Milligrams · mg", 1, 1000000),
            unit("Tonnes · t", 1000, 1),
            unit("Pounds · lb", 45359237, 100000000),
            unit("Ounces · oz", 45359237, 1600000000),
        ],
    },
    Category {
        name: "Temperature",
        units: &[
            Unit {
                name: "Celsius · °C",
                scale: (1, 1),
                offset: (27315, 100),
            },
            Unit {
                name: "Fahrenheit · °F",
                scale: (5, 9),
                offset: (45967, 180),
            },
            unit("Kelvin · K", 1, 1),
        ],
    },
    Category {
        name: "Area",
        units: &[
            unit("Square metres · m²", 1, 1),
            unit("Square kilometres · km²", 1000000, 1),
            unit("Hectares · ha", 10000, 1),
            unit("Square feet · ft²", 145161, 1562500),
            unit("Acres · ac", 316160658, 78125),
        ],
    },
    Category {
        name: "Volume",
        units: &[
            unit("Litres · L", 1, 1),
            unit("Millilitres · mL", 1, 1000),
            unit("Cubic metres · m³", 1000, 1),
            unit("US gallons · gal", 473176473, 125000000),
            unit("Imperial gallons · gal", 454609, 100000),
        ],
    },
    Category {
        name: "Time",
        units: &[
            unit("Seconds · s", 1, 1),
            unit("Milliseconds · ms", 1, 1000),
            unit("Minutes · min", 60, 1),
            unit("Hours · h", 3600, 1),
            unit("Days · d", 86400, 1),
            unit("Weeks", 604800, 1),
        ],
    },
    Category {
        name: "Speed",
        units: &[
            unit("Metres per second · m/s", 1, 1),
            unit("Kilometres per hour · km/h", 5, 18),
            unit("Miles per hour · mph", 1397, 3125),
        ],
    },
    Category {
        name: "Data",
        units: &[
            unit("Bytes · B", 1, 1),
            unit("Bits · bit", 1, 8),
            unit("Kilobytes · kB", 1000, 1),
            unit("Megabytes · MB", 1000000, 1),
            unit("Gigabytes · GB", 1000000000, 1),
            unit("Kibibytes · KiB", 1024, 1),
            unit("Mebibytes · MiB", 1048576, 1),
            unit("Gibibytes · GiB", 1073741824, 1),
        ],
    },
    Category {
        name: "Energy",
        units: &[
            unit("Joules · J", 1, 1),
            unit("Kilojoules · kJ", 1000, 1),
            unit("Watt-hours · Wh", 3600, 1),
            unit("Kilowatt-hours · kWh", 3600000, 1),
            unit("Thermochemical calories · cal", 523, 125),
        ],
    },
    Category {
        name: "Pressure",
        units: &[
            unit("Pascals · Pa", 1, 1),
            unit("Kilopascals · kPa", 1000, 1),
            unit("Bars · bar", 100000, 1),
            unit("Standard atmospheres · atm", 101325, 1),
        ],
    },
];
pub fn convert(input: Number, category: usize, from: usize, to: usize) -> Result<Number> {
    let category = CATEGORIES
        .get(category)
        .ok_or("Choose a conversion category")?;
    let from = category.units.get(from).ok_or("Choose a source unit")?;
    let to = category.units.get(to).ok_or("Choose a destination unit")?;
    let base = input
        .mul(Number::fraction(from.scale.0, from.scale.1)?)?
        .add(Number::fraction(from.offset.0, from.offset.1)?)?;
    if category.name == "Temperature" && base.float() < 0.0 {
        return Err("The temperature cannot be below absolute zero".into());
    }
    base.sub(Number::fraction(to.offset.0, to.offset.1)?)?
        .div(Number::fraction(to.scale.0, to.scale.1)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_conversions_are_exact() {
        assert_eq!(convert(Number::ONE, 0, 0, 1).unwrap().display(), "100");
        assert_eq!(
            convert(Number::Exact(32, 1), 2, 1, 0).unwrap().display(),
            "0"
        );
        assert_eq!(
            convert(Number::Exact(100, 1), 2, 0, 1).unwrap().display(),
            "212"
        );
        assert_eq!(convert(Number::ONE, 7, 5, 0).unwrap().display(), "1024");
        assert_eq!(
            convert(Number::ONE, 4, 3, 0).unwrap().display(),
            "3.785411784"
        );
    }
    #[test]
    fn every_pair_round_trips_without_binary_roundoff() {
        for (c, category) in CATEGORIES.iter().enumerate() {
            for from in 0..category.units.len() {
                for to in 0..category.units.len() {
                    let n = Number::Exact(300, 1);
                    let mapped = convert(n, c, from, to).unwrap();
                    assert_eq!(
                        convert(mapped, c, to, from).unwrap(),
                        n,
                        "{} {from} {to}",
                        category.name
                    );
                }
            }
        }
    }
    #[test]
    fn invalid_units_and_physical_temperature_are_rejected() {
        assert!(convert(Number::ONE, 100, 0, 0).is_err());
        assert!(convert(Number::Exact(-274, 1), 2, 0, 1).is_err());
        assert_eq!(convert(Number::ZERO, 2, 2, 0).unwrap().display(), "-273.15");
    }
}
