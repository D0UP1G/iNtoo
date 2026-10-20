//! Presentational GPUI periodic table for the ii Cheatsheet.
//!
//! The QML source is `modules/cheatsheet/CheatsheetPeriodicTable.qml` and
//! `ElementTile.qml`. The host owns clipboard integration and transient copy
//! confirmation; this view owns no service or global state.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, Role, ScrollHandle, SharedString, StatefulInteractiveElement, div,
    prelude::*, px, rgba,
};

use crate::theme::MaterialThemeContext;

pub type CopyElement = Arc<dyn Fn(String) + Send + Sync>;

/// The source table uses 18 columns. At this minimum width each cell remains
/// readable; narrower windows can scroll the table horizontally.
pub const PERIODIC_TABLE_MIN_WIDTH: f32 = 18. * 36. + 17. * 2. + 20.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementCategory {
    Metal,
    Nonmetal,
    Noblegas,
    Lanthanum,
    Actinium,
}

impl ElementCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Metal => "Metal",
            Self::Nonmetal => "Nonmetal",
            Self::Noblegas => "Noble gas",
            Self::Lanthanum => "Lanthanum",
            Self::Actinium => "Actinium",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    pub number: u8,
    pub name: &'static str,
    pub symbol: &'static str,
    pub weight: &'static str,
    pub category: ElementCategory,
    /// Zero-based visual row; 7 and 8 are the f-block rows.
    pub row: u8,
    /// Zero-based group (column), matching QML's 18-cell rows.
    pub column: u8,
}

/// Complete set of 118 source records, including the source's category
/// assignments and its separate lanthanide/actinide rows.
pub const ELEMENTS: &[Element] = &[
    Element {
        number: 1,
        name: "Hydrogen",
        symbol: "H",
        weight: "1.01",
        category: ElementCategory::Nonmetal,
        row: 0,
        column: 0,
    },
    Element {
        number: 2,
        name: "Helium",
        symbol: "He",
        weight: "4.00",
        category: ElementCategory::Noblegas,
        row: 0,
        column: 17,
    },
    Element {
        number: 3,
        name: "Lithium",
        symbol: "Li",
        weight: "6.94",
        category: ElementCategory::Metal,
        row: 1,
        column: 0,
    },
    Element {
        number: 4,
        name: "Beryllium",
        symbol: "Be",
        weight: "9.01",
        category: ElementCategory::Metal,
        row: 1,
        column: 1,
    },
    Element {
        number: 5,
        name: "Boron",
        symbol: "B",
        weight: "10.81",
        category: ElementCategory::Nonmetal,
        row: 1,
        column: 12,
    },
    Element {
        number: 6,
        name: "Carbon",
        symbol: "C",
        weight: "12.01",
        category: ElementCategory::Nonmetal,
        row: 1,
        column: 13,
    },
    Element {
        number: 7,
        name: "Nitrogen",
        symbol: "N",
        weight: "14.01",
        category: ElementCategory::Nonmetal,
        row: 1,
        column: 14,
    },
    Element {
        number: 8,
        name: "Oxygen",
        symbol: "O",
        weight: "16",
        category: ElementCategory::Nonmetal,
        row: 1,
        column: 15,
    },
    Element {
        number: 9,
        name: "Fluorine",
        symbol: "F",
        weight: "19",
        category: ElementCategory::Nonmetal,
        row: 1,
        column: 16,
    },
    Element {
        number: 10,
        name: "Neon",
        symbol: "Ne",
        weight: "20.18",
        category: ElementCategory::Noblegas,
        row: 1,
        column: 17,
    },
    Element {
        number: 11,
        name: "Sodium",
        symbol: "Na",
        weight: "22.99",
        category: ElementCategory::Metal,
        row: 2,
        column: 0,
    },
    Element {
        number: 12,
        name: "Magnesium",
        symbol: "Mg",
        weight: "24.31",
        category: ElementCategory::Metal,
        row: 2,
        column: 1,
    },
    Element {
        number: 13,
        name: "Aluminum",
        symbol: "Al",
        weight: "26.98",
        category: ElementCategory::Metal,
        row: 2,
        column: 12,
    },
    Element {
        number: 14,
        name: "Silicon",
        symbol: "Si",
        weight: "28.09",
        category: ElementCategory::Nonmetal,
        row: 2,
        column: 13,
    },
    Element {
        number: 15,
        name: "Phosphorus",
        symbol: "P",
        weight: "30.97",
        category: ElementCategory::Nonmetal,
        row: 2,
        column: 14,
    },
    Element {
        number: 16,
        name: "Sulfur",
        symbol: "S",
        weight: "32.07",
        category: ElementCategory::Nonmetal,
        row: 2,
        column: 15,
    },
    Element {
        number: 17,
        name: "Chlorine",
        symbol: "Cl",
        weight: "35.45",
        category: ElementCategory::Nonmetal,
        row: 2,
        column: 16,
    },
    Element {
        number: 18,
        name: "Argon",
        symbol: "Ar",
        weight: "39.95",
        category: ElementCategory::Noblegas,
        row: 2,
        column: 17,
    },
    Element {
        number: 19,
        name: "Potassium",
        symbol: "K",
        weight: "39.098",
        category: ElementCategory::Metal,
        row: 3,
        column: 0,
    },
    Element {
        number: 20,
        name: "Calcium",
        symbol: "Ca",
        weight: "40.078",
        category: ElementCategory::Metal,
        row: 3,
        column: 1,
    },
    Element {
        number: 21,
        name: "Scandium",
        symbol: "Sc",
        weight: "44.956",
        category: ElementCategory::Metal,
        row: 3,
        column: 2,
    },
    Element {
        number: 22,
        name: "Titanium",
        symbol: "Ti",
        weight: "47.87",
        category: ElementCategory::Metal,
        row: 3,
        column: 3,
    },
    Element {
        number: 23,
        name: "Vanadium",
        symbol: "V",
        weight: "50.94",
        category: ElementCategory::Metal,
        row: 3,
        column: 4,
    },
    Element {
        number: 24,
        name: "Chromium",
        symbol: "Cr",
        weight: "52",
        category: ElementCategory::Metal,
        row: 3,
        column: 5,
    },
    Element {
        number: 25,
        name: "Manganese",
        symbol: "Mn",
        weight: "54.94",
        category: ElementCategory::Metal,
        row: 3,
        column: 6,
    },
    Element {
        number: 26,
        name: "Iron",
        symbol: "Fe",
        weight: "55.85",
        category: ElementCategory::Metal,
        row: 3,
        column: 7,
    },
    Element {
        number: 27,
        name: "Cobalt",
        symbol: "Co",
        weight: "58.93",
        category: ElementCategory::Metal,
        row: 3,
        column: 8,
    },
    Element {
        number: 28,
        name: "Nickel",
        symbol: "Ni",
        weight: "58.69",
        category: ElementCategory::Metal,
        row: 3,
        column: 9,
    },
    Element {
        number: 29,
        name: "Copper",
        symbol: "Cu",
        weight: "63.55",
        category: ElementCategory::Metal,
        row: 3,
        column: 10,
    },
    Element {
        number: 30,
        name: "Zinc",
        symbol: "Zn",
        weight: "65.38",
        category: ElementCategory::Metal,
        row: 3,
        column: 11,
    },
    Element {
        number: 31,
        name: "Gallium",
        symbol: "Ga",
        weight: "69.72",
        category: ElementCategory::Metal,
        row: 3,
        column: 12,
    },
    Element {
        number: 32,
        name: "Germanium",
        symbol: "Ge",
        weight: "72.63",
        category: ElementCategory::Metal,
        row: 3,
        column: 13,
    },
    Element {
        number: 33,
        name: "Arsenic",
        symbol: "As",
        weight: "74.92",
        category: ElementCategory::Nonmetal,
        row: 3,
        column: 14,
    },
    Element {
        number: 34,
        name: "Selenium",
        symbol: "Se",
        weight: "78.96",
        category: ElementCategory::Nonmetal,
        row: 3,
        column: 15,
    },
    Element {
        number: 35,
        name: "Bromine",
        symbol: "Br",
        weight: "79.904",
        category: ElementCategory::Nonmetal,
        row: 3,
        column: 16,
    },
    Element {
        number: 36,
        name: "Krypton",
        symbol: "Kr",
        weight: "83.8",
        category: ElementCategory::Noblegas,
        row: 3,
        column: 17,
    },
    Element {
        number: 37,
        name: "Rubidium",
        symbol: "Rb",
        weight: "85.47",
        category: ElementCategory::Metal,
        row: 4,
        column: 0,
    },
    Element {
        number: 38,
        name: "Strontium",
        symbol: "Sr",
        weight: "87.62",
        category: ElementCategory::Metal,
        row: 4,
        column: 1,
    },
    Element {
        number: 39,
        name: "Yttrium",
        symbol: "Y",
        weight: "88.91",
        category: ElementCategory::Metal,
        row: 4,
        column: 2,
    },
    Element {
        number: 40,
        name: "Zirconium",
        symbol: "Zr",
        weight: "91.22",
        category: ElementCategory::Metal,
        row: 4,
        column: 3,
    },
    Element {
        number: 41,
        name: "Niobium",
        symbol: "Nb",
        weight: "92.91",
        category: ElementCategory::Metal,
        row: 4,
        column: 4,
    },
    Element {
        number: 42,
        name: "Molybdenum",
        symbol: "Mo",
        weight: "95.94",
        category: ElementCategory::Metal,
        row: 4,
        column: 5,
    },
    Element {
        number: 43,
        name: "Technetium",
        symbol: "Tc",
        weight: "98",
        category: ElementCategory::Metal,
        row: 4,
        column: 6,
    },
    Element {
        number: 44,
        name: "Ruthenium",
        symbol: "Ru",
        weight: "101.07",
        category: ElementCategory::Metal,
        row: 4,
        column: 7,
    },
    Element {
        number: 45,
        name: "Rhodium",
        symbol: "Rh",
        weight: "102.91",
        category: ElementCategory::Metal,
        row: 4,
        column: 8,
    },
    Element {
        number: 46,
        name: "Palladium",
        symbol: "Pd",
        weight: "106.42",
        category: ElementCategory::Metal,
        row: 4,
        column: 9,
    },
    Element {
        number: 47,
        name: "Silver",
        symbol: "Ag",
        weight: "107.87",
        category: ElementCategory::Metal,
        row: 4,
        column: 10,
    },
    Element {
        number: 48,
        name: "Cadmium",
        symbol: "Cd",
        weight: "112.41",
        category: ElementCategory::Metal,
        row: 4,
        column: 11,
    },
    Element {
        number: 49,
        name: "Indium",
        symbol: "In",
        weight: "114.82",
        category: ElementCategory::Metal,
        row: 4,
        column: 12,
    },
    Element {
        number: 50,
        name: "Tin",
        symbol: "Sn",
        weight: "118.71",
        category: ElementCategory::Metal,
        row: 4,
        column: 13,
    },
    Element {
        number: 51,
        name: "Antimony",
        symbol: "Sb",
        weight: "121.76",
        category: ElementCategory::Metal,
        row: 4,
        column: 14,
    },
    Element {
        number: 52,
        name: "Tellurium",
        symbol: "Te",
        weight: "127.6",
        category: ElementCategory::Nonmetal,
        row: 4,
        column: 15,
    },
    Element {
        number: 53,
        name: "Iodine",
        symbol: "I",
        weight: "126.9",
        category: ElementCategory::Nonmetal,
        row: 4,
        column: 16,
    },
    Element {
        number: 54,
        name: "Xenon",
        symbol: "Xe",
        weight: "131.29",
        category: ElementCategory::Noblegas,
        row: 4,
        column: 17,
    },
    Element {
        number: 55,
        name: "Cesium",
        symbol: "Cs",
        weight: "132.91",
        category: ElementCategory::Metal,
        row: 5,
        column: 0,
    },
    Element {
        number: 56,
        name: "Barium",
        symbol: "Ba",
        weight: "137.33",
        category: ElementCategory::Metal,
        row: 5,
        column: 1,
    },
    Element {
        number: 57,
        name: "Lanthanum",
        symbol: "La",
        weight: "138.91",
        category: ElementCategory::Lanthanum,
        row: 5,
        column: 2,
    },
    Element {
        number: 58,
        name: "Cerium",
        symbol: "Ce",
        weight: "140.12",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 3,
    },
    Element {
        number: 59,
        name: "Praseodymium",
        symbol: "Pr",
        weight: "140.91",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 4,
    },
    Element {
        number: 60,
        name: "Neodymium",
        symbol: "Nd",
        weight: "144.24",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 5,
    },
    Element {
        number: 61,
        name: "Promethium",
        symbol: "Pm",
        weight: "145",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 6,
    },
    Element {
        number: 62,
        name: "Samarium",
        symbol: "Sm",
        weight: "150.36",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 7,
    },
    Element {
        number: 63,
        name: "Europium",
        symbol: "Eu",
        weight: "151.96",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 8,
    },
    Element {
        number: 64,
        name: "Gadolinium",
        symbol: "Gd",
        weight: "157.25",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 9,
    },
    Element {
        number: 65,
        name: "Terbium",
        symbol: "Tb",
        weight: "158.93",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 10,
    },
    Element {
        number: 66,
        name: "Dysprosium",
        symbol: "Dy",
        weight: "162.5",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 11,
    },
    Element {
        number: 67,
        name: "Holmium",
        symbol: "Ho",
        weight: "164.93",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 12,
    },
    Element {
        number: 68,
        name: "Erbium",
        symbol: "Er",
        weight: "167.26",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 13,
    },
    Element {
        number: 69,
        name: "Thulium",
        symbol: "Tm",
        weight: "168.93",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 14,
    },
    Element {
        number: 70,
        name: "Ytterbium",
        symbol: "Yb",
        weight: "173.04",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 15,
    },
    Element {
        number: 71,
        name: "Lutetium",
        symbol: "Lu",
        weight: "174.97",
        category: ElementCategory::Lanthanum,
        row: 7,
        column: 16,
    },
    Element {
        number: 72,
        name: "Hafnium",
        symbol: "Hf",
        weight: "178.49",
        category: ElementCategory::Metal,
        row: 5,
        column: 3,
    },
    Element {
        number: 73,
        name: "Tantalum",
        symbol: "Ta",
        weight: "180.95",
        category: ElementCategory::Metal,
        row: 5,
        column: 4,
    },
    Element {
        number: 74,
        name: "Tungsten",
        symbol: "W",
        weight: "183.84",
        category: ElementCategory::Metal,
        row: 5,
        column: 5,
    },
    Element {
        number: 75,
        name: "Rhenium",
        symbol: "Re",
        weight: "186.21",
        category: ElementCategory::Metal,
        row: 5,
        column: 6,
    },
    Element {
        number: 76,
        name: "Osmium",
        symbol: "Os",
        weight: "190.23",
        category: ElementCategory::Metal,
        row: 5,
        column: 7,
    },
    Element {
        number: 77,
        name: "Iridium",
        symbol: "Ir",
        weight: "192.22",
        category: ElementCategory::Metal,
        row: 5,
        column: 8,
    },
    Element {
        number: 78,
        name: "Platinum",
        symbol: "Pt",
        weight: "195.09",
        category: ElementCategory::Metal,
        row: 5,
        column: 9,
    },
    Element {
        number: 79,
        name: "Gold",
        symbol: "Au",
        weight: "196.97",
        category: ElementCategory::Metal,
        row: 5,
        column: 10,
    },
    Element {
        number: 80,
        name: "Mercury",
        symbol: "Hg",
        weight: "200.59",
        category: ElementCategory::Metal,
        row: 5,
        column: 11,
    },
    Element {
        number: 81,
        name: "Thallium",
        symbol: "Tl",
        weight: "204.38",
        category: ElementCategory::Metal,
        row: 5,
        column: 12,
    },
    Element {
        number: 82,
        name: "Lead",
        symbol: "Pb",
        weight: "207.2",
        category: ElementCategory::Metal,
        row: 5,
        column: 13,
    },
    Element {
        number: 83,
        name: "Bismuth",
        symbol: "Bi",
        weight: "208.98",
        category: ElementCategory::Metal,
        row: 5,
        column: 14,
    },
    Element {
        number: 84,
        name: "Polonium",
        symbol: "Po",
        weight: "209",
        category: ElementCategory::Metal,
        row: 5,
        column: 15,
    },
    Element {
        number: 85,
        name: "Astatine",
        symbol: "At",
        weight: "210",
        category: ElementCategory::Nonmetal,
        row: 5,
        column: 16,
    },
    Element {
        number: 86,
        name: "Radon",
        symbol: "Rn",
        weight: "222",
        category: ElementCategory::Noblegas,
        row: 5,
        column: 17,
    },
    Element {
        number: 87,
        name: "Francium",
        symbol: "Fr",
        weight: "223",
        category: ElementCategory::Metal,
        row: 6,
        column: 0,
    },
    Element {
        number: 88,
        name: "Radium",
        symbol: "Ra",
        weight: "226",
        category: ElementCategory::Metal,
        row: 6,
        column: 1,
    },
    Element {
        number: 89,
        name: "Actinium",
        symbol: "Ac",
        weight: "227",
        category: ElementCategory::Actinium,
        row: 6,
        column: 2,
    },
    Element {
        number: 90,
        name: "Thorium",
        symbol: "Th",
        weight: "232.04",
        category: ElementCategory::Actinium,
        row: 8,
        column: 3,
    },
    Element {
        number: 91,
        name: "Protactinium",
        symbol: "Pa",
        weight: "231.04",
        category: ElementCategory::Actinium,
        row: 8,
        column: 4,
    },
    Element {
        number: 92,
        name: "Uranium",
        symbol: "U",
        weight: "238.03",
        category: ElementCategory::Actinium,
        row: 8,
        column: 5,
    },
    Element {
        number: 93,
        name: "Neptunium",
        symbol: "Np",
        weight: "237",
        category: ElementCategory::Actinium,
        row: 8,
        column: 6,
    },
    Element {
        number: 94,
        name: "Plutonium",
        symbol: "Pu",
        weight: "244",
        category: ElementCategory::Actinium,
        row: 8,
        column: 7,
    },
    Element {
        number: 95,
        name: "Americium",
        symbol: "Am",
        weight: "243",
        category: ElementCategory::Actinium,
        row: 8,
        column: 8,
    },
    Element {
        number: 96,
        name: "Curium",
        symbol: "Cm",
        weight: "247",
        category: ElementCategory::Actinium,
        row: 8,
        column: 9,
    },
    Element {
        number: 97,
        name: "Berkelium",
        symbol: "Bk",
        weight: "247",
        category: ElementCategory::Actinium,
        row: 8,
        column: 10,
    },
    Element {
        number: 98,
        name: "Californium",
        symbol: "Cf",
        weight: "251",
        category: ElementCategory::Actinium,
        row: 8,
        column: 11,
    },
    Element {
        number: 99,
        name: "Einsteinium",
        symbol: "Es",
        weight: "252",
        category: ElementCategory::Actinium,
        row: 8,
        column: 12,
    },
    Element {
        number: 100,
        name: "Fermium",
        symbol: "Fm",
        weight: "257",
        category: ElementCategory::Actinium,
        row: 8,
        column: 13,
    },
    Element {
        number: 101,
        name: "Mendelevium",
        symbol: "Md",
        weight: "258",
        category: ElementCategory::Actinium,
        row: 8,
        column: 14,
    },
    Element {
        number: 102,
        name: "Nobelium",
        symbol: "No",
        weight: "259",
        category: ElementCategory::Actinium,
        row: 8,
        column: 15,
    },
    Element {
        number: 103,
        name: "Lawrencium",
        symbol: "Lr",
        weight: "262",
        category: ElementCategory::Actinium,
        row: 8,
        column: 16,
    },
    Element {
        number: 104,
        name: "Rutherfordium",
        symbol: "Rf",
        weight: "267",
        category: ElementCategory::Metal,
        row: 6,
        column: 3,
    },
    Element {
        number: 105,
        name: "Dubnium",
        symbol: "Db",
        weight: "268",
        category: ElementCategory::Metal,
        row: 6,
        column: 4,
    },
    Element {
        number: 106,
        name: "Seaborgium",
        symbol: "Sg",
        weight: "271",
        category: ElementCategory::Metal,
        row: 6,
        column: 5,
    },
    Element {
        number: 107,
        name: "Bohrium",
        symbol: "Bh",
        weight: "272",
        category: ElementCategory::Metal,
        row: 6,
        column: 6,
    },
    Element {
        number: 108,
        name: "Hassium",
        symbol: "Hs",
        weight: "277",
        category: ElementCategory::Metal,
        row: 6,
        column: 7,
    },
    Element {
        number: 109,
        name: "Meitnerium",
        symbol: "Mt",
        weight: "278",
        category: ElementCategory::Metal,
        row: 6,
        column: 8,
    },
    Element {
        number: 110,
        name: "Darmstadtium",
        symbol: "Ds",
        weight: "281",
        category: ElementCategory::Metal,
        row: 6,
        column: 9,
    },
    Element {
        number: 111,
        name: "Roentgenium",
        symbol: "Rg",
        weight: "280",
        category: ElementCategory::Metal,
        row: 6,
        column: 10,
    },
    Element {
        number: 112,
        name: "Copernicium",
        symbol: "Cn",
        weight: "285",
        category: ElementCategory::Metal,
        row: 6,
        column: 11,
    },
    Element {
        number: 113,
        name: "Nihonium",
        symbol: "Nh",
        weight: "286",
        category: ElementCategory::Metal,
        row: 6,
        column: 12,
    },
    Element {
        number: 114,
        name: "Flerovium",
        symbol: "Fl",
        weight: "289",
        category: ElementCategory::Metal,
        row: 6,
        column: 13,
    },
    Element {
        number: 115,
        name: "Moscovium",
        symbol: "Mc",
        weight: "290",
        category: ElementCategory::Metal,
        row: 6,
        column: 14,
    },
    Element {
        number: 116,
        name: "Livermorium",
        symbol: "Lv",
        weight: "293",
        category: ElementCategory::Metal,
        row: 6,
        column: 15,
    },
    Element {
        number: 117,
        name: "Tennessine",
        symbol: "Ts",
        weight: "294",
        category: ElementCategory::Metal,
        row: 6,
        column: 16,
    },
    Element {
        number: 118,
        name: "Oganesson",
        symbol: "Og",
        weight: "294",
        category: ElementCategory::Noblegas,
        row: 6,
        column: 17,
    },
];

pub struct PeriodicTableInput {
    pub theme: MaterialThemeContext,
    pub scroll: ScrollHandle,
    /// Symbol currently shown as copied, supplied by the host if desired.
    pub copied_symbol: Option<String>,
    /// Host performs the actual clipboard write after a user activates a tile.
    pub on_copy: CopyElement,
}

impl std::fmt::Debug for PeriodicTableInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeriodicTableInput")
            .field("theme", &self.theme)
            .field("copied_symbol", &self.copied_symbol)
            .finish_non_exhaustive()
    }
}

/// Render the periodic-table page. Cell activation and keyboard activation
/// invoke the same host callback; all labels are localized by the host in a
/// later integration if the prototype adds translation resources.
pub fn render_periodic_table(input: PeriodicTableInput) -> AnyElement {
    let colors = &input.theme.colors;
    let surface = colors
        .surface_container_low
        .or(colors.surface)
        .unwrap_or_else(|| rgba(0xf4f1f8ff).into());
    let card = colors
        .surface_container
        .or(colors.surface_variant)
        .unwrap_or_else(|| rgba(0xece6f0ff).into());
    let foreground = colors
        .on_surface
        .or(input.theme.overrides.foreground)
        .unwrap_or_else(|| rgba(0x1b1b1fff).into());
    let accent = colors
        .primary
        .or(input.theme.overrides.accent)
        .unwrap_or_else(|| rgba(0x6750a4ff).into());
    let outline = colors
        .outline_variant
        .or(input.theme.overrides.border)
        .unwrap_or_else(|| rgba(0x79747e66).into());
    let scale = input.theme.typography.size_scale.max(1.0);
    let padding = input.theme.spacing.md.max(10.);
    let gap = 2.;
    let copied = input.copied_symbol.clone();
    let copied_status = copied.as_deref().map(copy_status_text);
    let on_copy = input.on_copy.clone();
    let scroll = input.scroll;

    let mut table = div()
        .id("ii.cheatsheet.periodic-table.scroller")
        .w_full()
        .min_w(px(PERIODIC_TABLE_MIN_WIDTH))
        .flex()
        .flex_col()
        .gap(px(gap))
        .role(Role::Grid)
        .aria_label("Periodic table of elements, 118 elements. Use Tab to focus an element and Enter or Space to copy its symbol.");

    for row_index in 0..9_u8 {
        let mut row = div()
            .id(SharedString::from(format!(
                "ii.cheatsheet.periodic-table.row-{row_index}"
            )))
            .w_full()
            .flex()
            .flex_row()
            .gap(px(gap))
            .role(Role::Row)
            .aria_label(match row_index {
                7 => "Lanthanides",
                8 => "Actinides",
                _ => "Periodic table period",
            });
        for column in 0..18_u8 {
            let element = ELEMENTS
                .iter()
                .find(|e| e.row == row_index && e.column == column)
                .copied();
            row = row.child(match element {
                Some(element) => element_tile(
                    element,
                    copied.as_deref() == Some(element.symbol),
                    &on_copy,
                    &input.theme,
                    scale,
                    outline,
                ),
                None => div().flex_1().min_w(px(34.)).h(px(52.)).into_any_element(),
            });
        }
        table = table.child(row);
        if row_index == 6 {
            table = table.child(div().h(px(6.)).w_full());
        }
    }

    div()
        .id("ii.cheatsheet.periodic-table")
        .w_full()
        .h_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(padding))
        .bg(surface)
        .role(Role::Group)
        .aria_label("Periodic table of elements")
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_lg()
                        .text_color(accent)
                        .child("experiment"),
                )
                .child(
                    div()
                        .id("ii.cheatsheet.periodic-table.heading")
                        .text_size(px(20. * scale))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .role(Role::Heading)
                        .aria_level(1)
                        .child("Periodic Table of Elements"),
                )
                .when_some(copied_status, |row, message| {
                    row.child(
                        div()
                            .id("ii.cheatsheet.periodic-table.copy-status")
                            .text_sm()
                            .text_color(accent)
                            .role(Role::Status)
                            .aria_label(SharedString::from(message.clone()))
                            .child(message),
                    )
                }),
        )
        .child(
            div()
                .id("ii.cheatsheet.periodic-table.card")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_x_scroll()
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .rounded(px(input.theme.shape.large))
                .border_1()
                .border_color(outline)
                .bg(card)
                .p(px(10.))
                .child(table),
        )
        .child(render_legend(accent, foreground, &input.theme, scale))
        .into_any_element()
}

fn element_tile(
    element: Element,
    copied: bool,
    on_copy: &CopyElement,
    theme: &MaterialThemeContext,
    scale: f32,
    outline: gpui::Hsla,
) -> AnyElement {
    let (background, text) = category_colors(element.category, theme);
    let label = format!(
        "{} ({}), atomic number {}, atomic weight {}, {}. {}",
        element.name,
        element.symbol,
        element.number,
        element.weight,
        element.category.label(),
        if copied {
            "Copied to clipboard."
        } else {
            "Activate to copy symbol."
        }
    );
    let symbol = element.symbol.to_owned();
    let keyboard_copy = on_copy.clone();
    div()
        .id(SharedString::from(format!(
            "ii.cheatsheet.element-{}",
            element.number
        )))
        .flex_1()
        .min_w(px(34.))
        .h(px(52.))
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.))
        .rounded(px(theme.shape.small))
        .bg(background)
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(SharedString::from(label))
        .focusable()
        .tab_stop(true)
        .focus_visible(move |style| style.border_2().border_color(outline))
        .on_click({
            let cb = on_copy.clone();
            let symbol = symbol.clone();
            move |_, _, _| cb(symbol.clone())
        })
        .on_key_down(move |event: &gpui::KeyDownEvent, _, cx| {
            if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                keyboard_copy(symbol.clone());
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(3.))
                .text_size(px(8. * scale))
                .text_color(text.opacity(0.82))
                .child(element.number.to_string()),
        )
        .child(
            div()
                .text_size(px(18. * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(text)
                .child(element.symbol),
        )
        .child(if copied {
            div()
                .text_size(px(7. * scale))
                .text_color(text)
                .child("Copied")
        } else {
            div()
                .text_size(px(7. * scale))
                .text_color(text.opacity(0.92))
                .child(element.name)
        })
        .into_any_element()
}

fn category_colors(
    category: ElementCategory,
    theme: &MaterialThemeContext,
) -> (gpui::Hsla, gpui::Hsla) {
    let colors = &theme.colors;
    let (background, foreground) = match category {
        ElementCategory::Metal => (colors.secondary, colors.on_primary),
        ElementCategory::Nonmetal => (colors.tertiary, colors.on_primary),
        ElementCategory::Noblegas => (colors.primary, colors.on_primary),
        ElementCategory::Lanthanum => (colors.primary_container, colors.on_primary_container),
        ElementCategory::Actinium => (colors.secondary_container, colors.on_secondary_container),
    };
    let bg = background
        .or(theme.overrides.accent)
        .unwrap_or_else(|| rgba(0x6750a4ff).into());
    let fg = foreground
        .or(theme.overrides.foreground)
        .unwrap_or_else(|| rgba(0xffffffff).into());
    (bg, fg)
}

fn copy_status_text(symbol: &str) -> String {
    format!("Copied {symbol} to clipboard")
}

fn render_legend(
    accent: gpui::Hsla,
    foreground: gpui::Hsla,
    theme: &MaterialThemeContext,
    scale: f32,
) -> AnyElement {
    let entries = [
        (ElementCategory::Metal, "Metal"),
        (ElementCategory::Nonmetal, "Nonmetal"),
        (ElementCategory::Noblegas, "Noble gas"),
        (ElementCategory::Lanthanum, "Lanthanum"),
        (ElementCategory::Actinium, "Actinium"),
    ];
    let mut legend = div()
        .id("ii.cheatsheet.periodic-table.legend")
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(theme.spacing.md))
        .role(Role::Group)
        .aria_label("Element categories");
    for (category, label) in entries {
        let (swatch, _) = category_colors(category, theme);
        legend = legend.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "ii.cheatsheet.periodic-table.legend.{label}"
                        )))
                        .size(px(14.))
                        .rounded(px(theme.shape.small))
                        .bg(swatch)
                        .aria_label(SharedString::from(format!("{label} category color"))),
                )
                .child(
                    div()
                        .text_size(px(12. * scale))
                        .text_color(foreground)
                        .child(label),
                ),
        );
    }
    // Keep this parameter explicit in the helper signature because the theme's
    // standard accent is also used by adjacent Cheatsheet Material pages.
    let _ = accent;
    legend.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn source_table_contains_all_unique_atomic_numbers_and_symbols() {
        assert_eq!(ELEMENTS.len(), 118);
        let numbers: HashSet<_> = ELEMENTS.iter().map(|element| element.number).collect();
        let symbols: HashSet<_> = ELEMENTS.iter().map(|element| element.symbol).collect();
        assert_eq!(numbers.len(), 118);
        assert_eq!(symbols.len(), 118);
        assert!(
            ELEMENTS
                .iter()
                .any(|element| element.number == 118 && element.symbol == "Og")
        );
    }

    #[test]
    fn element_cells_keep_valid_table_coordinates() {
        assert!(
            ELEMENTS
                .iter()
                .all(|element| element.row < 9 && element.column < 18)
        );
        assert!(
            ELEMENTS
                .iter()
                .all(|element| element.name.len() > 1 && !element.weight.is_empty())
        );
    }

    #[test]
    fn copy_feedback_has_an_explicit_accessible_status_message() {
        assert_eq!(copy_status_text("Fe"), "Copied Fe to clipboard");
    }
}
