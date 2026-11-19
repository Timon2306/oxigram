//! Country data for the phone login screen: dial codes, masks, flags, search.

static COUNTRIES: &[Country] = &[
    Country { iso: "RU", dial: "7", name: "Russia", mask: "000 000 00 00" },
    Country { iso: "UA", dial: "380", name: "Ukraine", mask: "00 000 00 00" },
    Country { iso: "BY", dial: "375", name: "Belarus", mask: "00 000 00 00" },
    Country { iso: "KZ", dial: "7", name: "Kazakhstan", mask: "000 000 00 00" },
    Country { iso: "US", dial: "1", name: "United States", mask: "000 000 0000" },
    Country { iso: "CA", dial: "1", name: "Canada", mask: "000 000 0000" },
    Country { iso: "AF", dial: "93", name: "Afghanistan", mask: "00 000 0000" },
    Country { iso: "AX", dial: "358", name: "Åland Islands", mask: "00 000 0000" },
    Country { iso: "AL", dial: "355", name: "Albania", mask: "00 000 0000" },
    Country { iso: "DZ", dial: "213", name: "Algeria", mask: "000 00 00 00" },
    Country { iso: "AS", dial: "1", name: "American Samoa", mask: "000 000 0000" },
    Country { iso: "AD", dial: "376", name: "Andorra", mask: "000 000" },
    Country { iso: "AO", dial: "244", name: "Angola", mask: "000 000 000" },
    Country { iso: "AI", dial: "1", name: "Anguilla", mask: "000 000 0000" },
    Country { iso: "AG", dial: "1", name: "Antigua & Barbuda", mask: "000 000 0000" },
    Country { iso: "AR", dial: "54", name: "Argentina", mask: "00 0000 0000" },
    Country { iso: "AM", dial: "374", name: "Armenia", mask: "00 000000" },
    Country { iso: "AW", dial: "297", name: "Aruba", mask: "000 0000" },
    Country { iso: "AU", dial: "61", name: "Australia", mask: "0 0000 0000" },
    Country { iso: "AT", dial: "43", name: "Austria", mask: "000 000000" },
    Country { iso: "AZ", dial: "994", name: "Azerbaijan", mask: "00 000 00 00" },
    Country { iso: "BS", dial: "1", name: "Bahamas", mask: "000 000 0000" },
    Country { iso: "BH", dial: "973", name: "Bahrain", mask: "0000 0000" },
    Country { iso: "BD", dial: "880", name: "Bangladesh", mask: "0000 000000" },
    Country { iso: "BB", dial: "1", name: "Barbados", mask: "000 000 0000" },
    Country { iso: "BE", dial: "32", name: "Belgium", mask: "000 00 00 00" },
    Country { iso: "BZ", dial: "501", name: "Belize", mask: "000 0000" },
    Country { iso: "BJ", dial: "229", name: "Benin", mask: "00 00 00 00" },
    Country { iso: "BM", dial: "1", name: "Bermuda", mask: "000 000 0000" },
    Country { iso: "BT", dial: "975", name: "Bhutan", mask: "00 000 000" },
    Country { iso: "BO", dial: "591", name: "Bolivia", mask: "0 000 0000" },
    Country { iso: "BA", dial: "387", name: "Bosnia & Herzegovina", mask: "00 000 000" },
    Country { iso: "BW", dial: "267", name: "Botswana", mask: "00 000 000" },
    Country { iso: "BR", dial: "55", name: "Brazil", mask: "00 00000 0000" },
    Country { iso: "VG", dial: "1", name: "British Virgin Islands", mask: "000 000 0000" },
    Country { iso: "BN", dial: "673", name: "Brunei", mask: "000 0000" },
    Country { iso: "BG", dial: "359", name: "Bulgaria", mask: "000 000 000" },
    Country { iso: "BF", dial: "226", name: "Burkina Faso", mask: "00 00 00 00" },
    Country { iso: "BI", dial: "257", name: "Burundi", mask: "00 00 00 00" },
    Country { iso: "KH", dial: "855", name: "Cambodia", mask: "00 000 000" },
    Country { iso: "CM", dial: "237", name: "Cameroon", mask: "0 00 00 00 00" },
    Country { iso: "CV", dial: "238", name: "Cape Verde", mask: "000 00 00" },
    Country { iso: "BQ", dial: "599", name: "Caribbean Netherlands", mask: "000 0000" },
    Country { iso: "KY", dial: "1", name: "Cayman Islands", mask: "000 000 0000" },
    Country { iso: "CF", dial: "236", name: "Central African Republic", mask: "00 00 00 00" },
    Country { iso: "TD", dial: "235", name: "Chad", mask: "00 00 00 00" },
    Country { iso: "CL", dial: "56", name: "Chile", mask: "0 0000 0000" },
    Country { iso: "CN", dial: "86", name: "China", mask: "000 0000 0000" },
    Country { iso: "CX", dial: "61", name: "Christmas Island", mask: "0 0000 0000" },
    Country { iso: "CC", dial: "61", name: "Cocos (Keeling) Islands", mask: "0 0000 0000" },
    Country { iso: "CO", dial: "57", name: "Colombia", mask: "000 0000000" },
    Country { iso: "KM", dial: "269", name: "Comoros", mask: "000 00 00" },
    Country { iso: "CG", dial: "242", name: "Congo - Brazzaville", mask: "00 000 0000" },
    Country { iso: "CD", dial: "243", name: "Congo - Kinshasa", mask: "000 000 000" },
    Country { iso: "CK", dial: "682", name: "Cook Islands", mask: "00 000" },
    Country { iso: "CR", dial: "506", name: "Costa Rica", mask: "0000 0000" },
    Country { iso: "CI", dial: "225", name: "Côte d’Ivoire", mask: "00 00 00 00 00" },
    Country { iso: "HR", dial: "385", name: "Croatia", mask: "00 000 0000" },
    Country { iso: "CU", dial: "53", name: "Cuba", mask: "0 000 0000" },
    Country { iso: "CW", dial: "599", name: "Curaçao", mask: "0 000 0000" },
    Country { iso: "CY", dial: "357", name: "Cyprus", mask: "00 000000" },
    Country { iso: "CZ", dial: "420", name: "Czechia", mask: "000 000 000" },
    Country { iso: "DK", dial: "45", name: "Denmark", mask: "00 00 00 00" },
    Country { iso: "DJ", dial: "253", name: "Djibouti", mask: "00 00 00 00" },
    Country { iso: "DM", dial: "1", name: "Dominica", mask: "000 000 0000" },
    Country { iso: "DO", dial: "1", name: "Dominican Republic", mask: "000 000 0000" },
    Country { iso: "EC", dial: "593", name: "Ecuador", mask: "00 000 0000" },
    Country { iso: "EG", dial: "20", name: "Egypt", mask: "00 0000 0000" },
    Country { iso: "SV", dial: "503", name: "El Salvador", mask: "0000 0000" },
    Country { iso: "GQ", dial: "240", name: "Equatorial Guinea", mask: "000 000 000" },
    Country { iso: "ER", dial: "291", name: "Eritrea", mask: "0 000 000" },
    Country { iso: "EE", dial: "372", name: "Estonia", mask: "0000 0000" },
    Country { iso: "SZ", dial: "268", name: "Eswatini", mask: "0000 0000" },
    Country { iso: "ET", dial: "251", name: "Ethiopia", mask: "00 000 0000" },
    Country { iso: "FK", dial: "500", name: "Falkland Islands", mask: "00000" },
    Country { iso: "FO", dial: "298", name: "Faroe Islands", mask: "000000" },
    Country { iso: "FJ", dial: "679", name: "Fiji", mask: "000 0000" },
    Country { iso: "FI", dial: "358", name: "Finland", mask: "00 000 0000" },
    Country { iso: "FR", dial: "33", name: "France", mask: "0 00 00 00 00" },
    Country { iso: "GF", dial: "594", name: "French Guiana", mask: "000 00 00 00" },
    Country { iso: "PF", dial: "689", name: "French Polynesia", mask: "00 00 00 00" },
    Country { iso: "GA", dial: "241", name: "Gabon", mask: "0 00 00 00" },
    Country { iso: "GM", dial: "220", name: "Gambia", mask: "000 0000" },
    Country { iso: "GE", dial: "995", name: "Georgia", mask: "000 00 00 00" },
    Country { iso: "DE", dial: "49", name: "Germany", mask: "000 00000000" },
    Country { iso: "GH", dial: "233", name: "Ghana", mask: "00 000 0000" },
    Country { iso: "GI", dial: "350", name: "Gibraltar", mask: "000 00000" },
    Country { iso: "GR", dial: "30", name: "Greece", mask: "000 000 0000" },
    Country { iso: "GL", dial: "299", name: "Greenland", mask: "00 00 00" },
    Country { iso: "GD", dial: "1", name: "Grenada", mask: "000 000 0000" },
    Country { iso: "GP", dial: "590", name: "Guadeloupe", mask: "000 00 00 00" },
    Country { iso: "GU", dial: "1", name: "Guam", mask: "000 000 0000" },
    Country { iso: "GT", dial: "502", name: "Guatemala", mask: "0000 0000" },
    Country { iso: "GB", dial: "44", name: "United Kingdom", mask: "0000 000000" },
    Country { iso: "GG", dial: "44", name: "Guernsey", mask: "0000 000000" },
    Country { iso: "GN", dial: "224", name: "Guinea", mask: "000 00 00 00" },
    Country { iso: "GW", dial: "245", name: "Guinea-Bissau", mask: "000 000 000" },
    Country { iso: "GY", dial: "592", name: "Guyana", mask: "000 0000" },
    Country { iso: "HT", dial: "509", name: "Haiti", mask: "00 00 0000" },
    Country { iso: "HN", dial: "504", name: "Honduras", mask: "0000 0000" },
    Country { iso: "HK", dial: "852", name: "Hong Kong", mask: "0000 0000" },
    Country { iso: "HU", dial: "36", name: "Hungary", mask: "00 000 0000" },
    Country { iso: "IS", dial: "354", name: "Iceland", mask: "000 0000" },
    Country { iso: "IN", dial: "91", name: "India", mask: "00000 00000" },
    Country { iso: "ID", dial: "62", name: "Indonesia", mask: "000 0000 0000" },
    Country { iso: "IR", dial: "98", name: "Iran", mask: "000 000 0000" },
    Country { iso: "IQ", dial: "964", name: "Iraq", mask: "000 000 0000" },
    Country { iso: "IE", dial: "353", name: "Ireland", mask: "00 000 0000" },
    Country { iso: "IM", dial: "44", name: "Isle of Man", mask: "0000 000000" },
    Country { iso: "IL", dial: "972", name: "Israel", mask: "00 000 0000" },
    Country { iso: "IT", dial: "39", name: "Italy", mask: "000 000 0000" },
    Country { iso: "JM", dial: "1", name: "Jamaica", mask: "000 000 0000" },
    Country { iso: "JP", dial: "81", name: "Japan", mask: "00 0000 0000" },
    Country { iso: "JE", dial: "44", name: "Jersey", mask: "0000 000000" },
    Country { iso: "JO", dial: "962", name: "Jordan", mask: "0 0000 0000" },
    Country { iso: "KE", dial: "254", name: "Kenya", mask: "000 000000" },
    Country { iso: "KI", dial: "686", name: "Kiribati", mask: "00000000" },
    Country { iso: "XK", dial: "383", name: "Kosovo", mask: "00 000 000" },
    Country { iso: "KW", dial: "965", name: "Kuwait", mask: "000 00000" },
    Country { iso: "KG", dial: "996", name: "Kyrgyzstan", mask: "000 00 00 00" },
    Country { iso: "LA", dial: "856", name: "Laos", mask: "00 00 000 000" },
    Country { iso: "LV", dial: "371", name: "Latvia", mask: "0000 0000" },
    Country { iso: "LB", dial: "961", name: "Lebanon", mask: "00 000 000" },
    Country { iso: "LS", dial: "266", name: "Lesotho", mask: "0000 0000" },
    Country { iso: "LR", dial: "231", name: "Liberia", mask: "00 000 0000" },
    Country { iso: "LY", dial: "218", name: "Libya", mask: "00 0000000" },
    Country { iso: "LI", dial: "423", name: "Liechtenstein", mask: "000 000 000" },
    Country { iso: "LT", dial: "370", name: "Lithuania", mask: "000 00000" },
    Country { iso: "LU", dial: "352", name: "Luxembourg", mask: "000 000 000" },
    Country { iso: "MO", dial: "853", name: "Macao", mask: "0000 0000" },
    Country { iso: "MG", dial: "261", name: "Madagascar", mask: "00 00 000 00" },
    Country { iso: "MW", dial: "265", name: "Malawi", mask: "0 0000 0000" },
    Country { iso: "MY", dial: "60", name: "Malaysia", mask: "00 000 0000" },
    Country { iso: "MV", dial: "960", name: "Maldives", mask: "000 0000" },
    Country { iso: "ML", dial: "223", name: "Mali", mask: "00 00 00 00" },
    Country { iso: "MT", dial: "356", name: "Malta", mask: "0000 0000" },
    Country { iso: "MH", dial: "692", name: "Marshall Islands", mask: "000 0000" },
    Country { iso: "MQ", dial: "596", name: "Martinique", mask: "000 00 00 00" },
    Country { iso: "MR", dial: "222", name: "Mauritania", mask: "00 00 00 00" },
    Country { iso: "MU", dial: "230", name: "Mauritius", mask: "0000 0000" },
    Country { iso: "YT", dial: "262", name: "Mayotte", mask: "000 00 00 00" },
    Country { iso: "MX", dial: "52", name: "Mexico", mask: "00 0000 0000" },
    Country { iso: "FM", dial: "691", name: "Micronesia", mask: "000 0000" },
    Country { iso: "MD", dial: "373", name: "Moldova", mask: "0000 0000" },
    Country { iso: "MC", dial: "377", name: "Monaco", mask: "00 00 00 00" },
    Country { iso: "MN", dial: "976", name: "Mongolia", mask: "0000 0000" },
    Country { iso: "ME", dial: "382", name: "Montenegro", mask: "00 000 000" },
    Country { iso: "MS", dial: "1", name: "Montserrat", mask: "000 000 0000" },
    Country { iso: "MA", dial: "212", name: "Morocco", mask: "00 0000000" },
    Country { iso: "MZ", dial: "258", name: "Mozambique", mask: "00 000 0000" },
    Country { iso: "MM", dial: "95", name: "Myanmar", mask: "00 000 0000" },
    Country { iso: "NA", dial: "264", name: "Namibia", mask: "00 000 0000" },
    Country { iso: "NR", dial: "674", name: "Nauru", mask: "000 0000" },
    Country { iso: "NP", dial: "977", name: "Nepal", mask: "00 0000 0000" },
    Country { iso: "NL", dial: "31", name: "Netherlands", mask: "0 00000000" },
    Country { iso: "NC", dial: "687", name: "New Caledonia", mask: "00 00 00" },
    Country { iso: "NZ", dial: "64", name: "New Zealand", mask: "00 000 0000" },
    Country { iso: "NI", dial: "505", name: "Nicaragua", mask: "0000 0000" },
    Country { iso: "NE", dial: "227", name: "Niger", mask: "00 00 00 00" },
    Country { iso: "NG", dial: "234", name: "Nigeria", mask: "000 000 0000" },
    Country { iso: "NU", dial: "683", name: "Niue", mask: "0000" },
    Country { iso: "NF", dial: "672", name: "Norfolk Island", mask: "00 0000" },
    Country { iso: "KP", dial: "850", name: "North Korea", mask: "000 0000 0000" },
    Country { iso: "MK", dial: "389", name: "North Macedonia", mask: "00 000 000" },
    Country { iso: "MP", dial: "1", name: "Northern Mariana Islands", mask: "000 000 0000" },
    Country { iso: "NO", dial: "47", name: "Norway", mask: "000 00 000" },
    Country { iso: "SJ", dial: "47", name: "Svalbard & Jan Mayen", mask: "000 00 000" },
    Country { iso: "OM", dial: "968", name: "Oman", mask: "0000 0000" },
    Country { iso: "PK", dial: "92", name: "Pakistan", mask: "000 0000000" },
    Country { iso: "PW", dial: "680", name: "Palau", mask: "000 0000" },
    Country { iso: "PS", dial: "970", name: "Palestine", mask: "000 000 000" },
    Country { iso: "PA", dial: "507", name: "Panama", mask: "0000 0000" },
    Country { iso: "PG", dial: "675", name: "Papua New Guinea", mask: "0000 0000" },
    Country { iso: "PY", dial: "595", name: "Paraguay", mask: "000 000000" },
    Country { iso: "PE", dial: "51", name: "Peru", mask: "000 000 000" },
    Country { iso: "PH", dial: "63", name: "Philippines", mask: "000 000 0000" },
    Country { iso: "PL", dial: "48", name: "Poland", mask: "000 000 000" },
    Country { iso: "PT", dial: "351", name: "Portugal", mask: "000 000 000" },
    Country { iso: "PR", dial: "1", name: "Puerto Rico", mask: "000 000 0000" },
    Country { iso: "QA", dial: "974", name: "Qatar", mask: "0000 0000" },
    Country { iso: "RE", dial: "262", name: "Réunion", mask: "000 00 00 00" },
    Country { iso: "RO", dial: "40", name: "Romania", mask: "000 000 000" },
    Country { iso: "RW", dial: "250", name: "Rwanda", mask: "000 000 000" },
    Country { iso: "WS", dial: "685", name: "Samoa", mask: "00 00000" },
    Country { iso: "SM", dial: "378", name: "San Marino", mask: "0000 000000" },
    Country { iso: "ST", dial: "239", name: "São Tomé & Príncipe", mask: "000 0000" },
    Country { iso: "SA", dial: "966", name: "Saudi Arabia", mask: "00 000 0000" },
    Country { iso: "SN", dial: "221", name: "Senegal", mask: "00 000 00 00" },
    Country { iso: "RS", dial: "381", name: "Serbia", mask: "00 000 0000" },
    Country { iso: "SC", dial: "248", name: "Seychelles", mask: "0 000 000" },
    Country { iso: "SL", dial: "232", name: "Sierra Leone", mask: "00 000000" },
    Country { iso: "SG", dial: "65", name: "Singapore", mask: "0000 0000" },
    Country { iso: "SX", dial: "1", name: "Sint Maarten", mask: "000 000 0000" },
    Country { iso: "SK", dial: "421", name: "Slovakia", mask: "000 000 000" },
    Country { iso: "SI", dial: "386", name: "Slovenia", mask: "00 000 000" },
    Country { iso: "SB", dial: "677", name: "Solomon Islands", mask: "00000" },
    Country { iso: "SO", dial: "252", name: "Somalia", mask: "00 000 000" },
    Country { iso: "ZA", dial: "27", name: "South Africa", mask: "00 000 0000" },
    Country { iso: "KR", dial: "82", name: "South Korea", mask: "00 0000 0000" },
    Country { iso: "SS", dial: "211", name: "South Sudan", mask: "000 000 000" },
    Country { iso: "ES", dial: "34", name: "Spain", mask: "000 00 00 00" },
    Country { iso: "LK", dial: "94", name: "Sri Lanka", mask: "00 000 0000" },
    Country { iso: "BL", dial: "590", name: "St. Barthélemy", mask: "000 00 00 00" },
    Country { iso: "SH", dial: "290", name: "St. Helena", mask: "00000" },
    Country { iso: "KN", dial: "1", name: "St. Kitts & Nevis", mask: "000 000 0000" },
    Country { iso: "LC", dial: "1", name: "St. Lucia", mask: "000 000 0000" },
    Country { iso: "MF", dial: "590", name: "St. Martin", mask: "000 00 00 00" },
    Country { iso: "PM", dial: "508", name: "St. Pierre & Miquelon", mask: "00 00 00" },
    Country { iso: "VC", dial: "1", name: "St. Vincent & Grenadines", mask: "000 000 0000" },
    Country { iso: "SD", dial: "249", name: "Sudan", mask: "00 000 0000" },
    Country { iso: "SR", dial: "597", name: "Suriname", mask: "000 0000" },
    Country { iso: "SE", dial: "46", name: "Sweden", mask: "00 000 00 00" },
    Country { iso: "CH", dial: "41", name: "Switzerland", mask: "00 000 00 00" },
    Country { iso: "SY", dial: "963", name: "Syria", mask: "000 000 000" },
    Country { iso: "TW", dial: "886", name: "Taiwan", mask: "000 000 000" },
    Country { iso: "TJ", dial: "992", name: "Tajikistan", mask: "00 000 0000" },
    Country { iso: "TZ", dial: "255", name: "Tanzania", mask: "000 000 000" },
    Country { iso: "TH", dial: "66", name: "Thailand", mask: "00 000 0000" },
    Country { iso: "TL", dial: "670", name: "Timor-Leste", mask: "0000 0000" },
    Country { iso: "TG", dial: "228", name: "Togo", mask: "00 00 00 00" },
    Country { iso: "TK", dial: "690", name: "Tokelau", mask: "0000" },
    Country { iso: "TO", dial: "676", name: "Tonga", mask: "00000" },
    Country { iso: "TT", dial: "1", name: "Trinidad & Tobago", mask: "000 000 0000" },
    Country { iso: "TN", dial: "216", name: "Tunisia", mask: "00 000 000" },
    Country { iso: "TR", dial: "90", name: "Turkey", mask: "000 000 00 00" },
    Country { iso: "TM", dial: "993", name: "Turkmenistan", mask: "00 000000" },
    Country { iso: "TC", dial: "1", name: "Turks & Caicos Islands", mask: "000 000 0000" },
    Country { iso: "TV", dial: "688", name: "Tuvalu", mask: "00000" },
    Country { iso: "VI", dial: "1", name: "U.S. Virgin Islands", mask: "000 000 0000" },
    Country { iso: "UG", dial: "256", name: "Uganda", mask: "000 000000" },
    Country { iso: "AE", dial: "971", name: "United Arab Emirates", mask: "00 000 0000" },
    Country { iso: "UY", dial: "598", name: "Uruguay", mask: "0000 0000" },
    Country { iso: "UZ", dial: "998", name: "Uzbekistan", mask: "00 000 00 00" },
    Country { iso: "VU", dial: "678", name: "Vanuatu", mask: "00000" },
    Country { iso: "VA", dial: "39", name: "Vatican City", mask: "000 000 0000" },
    Country { iso: "VE", dial: "58", name: "Venezuela", mask: "000 0000000" },
    Country { iso: "VN", dial: "84", name: "Vietnam", mask: "00 0000 0000" },
    Country { iso: "WF", dial: "681", name: "Wallis & Futuna", mask: "00 00 00" },
    Country { iso: "EH", dial: "212", name: "Western Sahara", mask: "00 0000000" },
    Country { iso: "YE", dial: "967", name: "Yemen", mask: "000 000 000" },
    Country { iso: "ZM", dial: "260", name: "Zambia", mask: "00 000 0000" },
    Country { iso: "ZW", dial: "263", name: "Zimbabwe", mask: "00 000 0000" },
];

/// Country with its dial code and national-number mask.
#[derive(Debug, PartialEq, Eq)]
pub struct Country {
    pub iso: &'static str,
    pub dial: &'static str,
    pub name: &'static str,
    pub mask: &'static str,
}

pub fn all() -> &'static [Country] {
    COUNTRIES
}

pub fn by_iso(iso: &str) -> Option<&'static Country> {
    COUNTRIES.iter().find(|c| c.iso.eq_ignore_ascii_case(iso.trim()))
}

pub fn from_locale(locale: &str) -> Option<&'static Country> {
    let base = locale.split(['.', '@']).next().unwrap_or("");
    let mut parts = base.split(['-', '_']);
    let lang = parts.next().unwrap_or("").to_ascii_lowercase();
    for part in parts {
        if part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()) {
            if let Some(c) = by_iso(part) {
                return Some(c);
            }
        }
    }
    let iso = match lang.as_str() {
        "ru" => "RU",
        "uk" => "UA",
        "be" => "BY",
        "kk" => "KZ",
        _ => return None,
    };
    by_iso(iso)
}

/// Only the ASCII digits of `text`.
pub fn digits(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Splits a full international number into country and national digits.
pub fn split(full: &str) -> Option<(&'static Country, String)> {
    let d = digits(full);
    for len in (1..=4).rev() {
        if d.len() < len {
            continue;
        }
        let (code, rest) = d.split_at(len);
        let mut found = COUNTRIES.iter().filter(|c| c.dial == code);
        let Some(first) = found.next() else { continue };
        let country = if code == "7" && (rest.starts_with('6') || rest.starts_with('7')) {
            by_iso("KZ").unwrap_or(first)
        } else {
            first
        };
        return Some((country, rest.to_string()));
    }
    None
}

/// Groups national digits by the country mask; extra digits are appended as is.
pub fn format(country: &Country, digits: &str) -> String {
    let mut out = String::new();
    let mut it = digits.chars().peekable();
    for m in country.mask.chars() {
        if it.peek().is_none() {
            return out;
        }
        if m == '0' {
            out.push(it.next().unwrap());
        } else {
            out.push(m);
        }
    }
    out.extend(it);
    out
}

/// Countries matching `query` by translated name, English name, ISO or dial code.
pub fn search(
    query: &str,
    translated_name: impl Fn(&'static Country) -> String,
) -> Vec<&'static Country> {
    let q = query.trim().to_lowercase();
    let dial_q: String = q.trim_start_matches('+').chars().filter(|c| !c.is_whitespace()).collect();
    let numeric = !dial_q.is_empty() && dial_q.chars().all(|c| c.is_ascii_digit());
    let mut list: Vec<(String, &'static Country)> = COUNTRIES
        .iter()
        .filter(|c| {
            q.is_empty()
                || (numeric && c.dial.starts_with(&dial_q))
                || c.iso.eq_ignore_ascii_case(&q)
                || c.name.to_lowercase().contains(&q)
                || translated_name(c).to_lowercase().contains(&q)
        })
        .map(|c| (translated_name(c).to_lowercase(), c))
        .collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list.into_iter().map(|(_, c)| c).collect()
}

/// Flag emoji built from regional indicator symbols.
pub fn flag(iso: &str) -> String {
    iso.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .filter_map(|c| char::from_u32(0x1F1E6 + (c.to_ascii_uppercase() as u32 - 'A' as u32)))
        .collect()
}

/// Every country name, marked for the translation tools.
#[cfg(feature = "i18n_marks")]
#[allow(dead_code)]
fn names_for_translation() {
    let _ = [
        oxigram_core::tr!("Russia"),
        oxigram_core::tr!("Ukraine"),
        oxigram_core::tr!("Belarus"),
        oxigram_core::tr!("Kazakhstan"),
        oxigram_core::tr!("United States"),
        oxigram_core::tr!("Canada"),
        oxigram_core::tr!("Afghanistan"),
        oxigram_core::tr!("Åland Islands"),
        oxigram_core::tr!("Albania"),
        oxigram_core::tr!("Algeria"),
        oxigram_core::tr!("American Samoa"),
        oxigram_core::tr!("Andorra"),
        oxigram_core::tr!("Angola"),
        oxigram_core::tr!("Anguilla"),
        oxigram_core::tr!("Antigua & Barbuda"),
        oxigram_core::tr!("Argentina"),
        oxigram_core::tr!("Armenia"),
        oxigram_core::tr!("Aruba"),
        oxigram_core::tr!("Australia"),
        oxigram_core::tr!("Austria"),
        oxigram_core::tr!("Azerbaijan"),
        oxigram_core::tr!("Bahamas"),
        oxigram_core::tr!("Bahrain"),
        oxigram_core::tr!("Bangladesh"),
        oxigram_core::tr!("Barbados"),
        oxigram_core::tr!("Belgium"),
        oxigram_core::tr!("Belize"),
        oxigram_core::tr!("Benin"),
        oxigram_core::tr!("Bermuda"),
        oxigram_core::tr!("Bhutan"),
        oxigram_core::tr!("Bolivia"),
        oxigram_core::tr!("Bosnia & Herzegovina"),
        oxigram_core::tr!("Botswana"),
        oxigram_core::tr!("Brazil"),
        oxigram_core::tr!("British Virgin Islands"),
        oxigram_core::tr!("Brunei"),
        oxigram_core::tr!("Bulgaria"),
        oxigram_core::tr!("Burkina Faso"),
        oxigram_core::tr!("Burundi"),
        oxigram_core::tr!("Cambodia"),
        oxigram_core::tr!("Cameroon"),
        oxigram_core::tr!("Cape Verde"),
        oxigram_core::tr!("Caribbean Netherlands"),
        oxigram_core::tr!("Cayman Islands"),
        oxigram_core::tr!("Central African Republic"),
        oxigram_core::tr!("Chad"),
        oxigram_core::tr!("Chile"),
        oxigram_core::tr!("China"),
        oxigram_core::tr!("Christmas Island"),
        oxigram_core::tr!("Cocos (Keeling) Islands"),
        oxigram_core::tr!("Colombia"),
        oxigram_core::tr!("Comoros"),
        oxigram_core::tr!("Congo - Brazzaville"),
        oxigram_core::tr!("Congo - Kinshasa"),
        oxigram_core::tr!("Cook Islands"),
        oxigram_core::tr!("Costa Rica"),
        oxigram_core::tr!("Côte d’Ivoire"),
        oxigram_core::tr!("Croatia"),
        oxigram_core::tr!("Cuba"),
        oxigram_core::tr!("Curaçao"),
        oxigram_core::tr!("Cyprus"),
        oxigram_core::tr!("Czechia"),
        oxigram_core::tr!("Denmark"),
        oxigram_core::tr!("Djibouti"),
        oxigram_core::tr!("Dominica"),
        oxigram_core::tr!("Dominican Republic"),
        oxigram_core::tr!("Ecuador"),
        oxigram_core::tr!("Egypt"),
        oxigram_core::tr!("El Salvador"),
        oxigram_core::tr!("Equatorial Guinea"),
        oxigram_core::tr!("Eritrea"),
        oxigram_core::tr!("Estonia"),
        oxigram_core::tr!("Eswatini"),
        oxigram_core::tr!("Ethiopia"),
        oxigram_core::tr!("Falkland Islands"),
        oxigram_core::tr!("Faroe Islands"),
        oxigram_core::tr!("Fiji"),
        oxigram_core::tr!("Finland"),
        oxigram_core::tr!("France"),
        oxigram_core::tr!("French Guiana"),
        oxigram_core::tr!("French Polynesia"),
        oxigram_core::tr!("Gabon"),
        oxigram_core::tr!("Gambia"),
        oxigram_core::tr!("Georgia"),
        oxigram_core::tr!("Germany"),
        oxigram_core::tr!("Ghana"),
        oxigram_core::tr!("Gibraltar"),
        oxigram_core::tr!("Greece"),
        oxigram_core::tr!("Greenland"),
        oxigram_core::tr!("Grenada"),
        oxigram_core::tr!("Guadeloupe"),
        oxigram_core::tr!("Guam"),
        oxigram_core::tr!("Guatemala"),
        oxigram_core::tr!("United Kingdom"),
        oxigram_core::tr!("Guernsey"),
        oxigram_core::tr!("Guinea"),
        oxigram_core::tr!("Guinea-Bissau"),
        oxigram_core::tr!("Guyana"),
        oxigram_core::tr!("Haiti"),
        oxigram_core::tr!("Honduras"),
        oxigram_core::tr!("Hong Kong"),
        oxigram_core::tr!("Hungary"),
        oxigram_core::tr!("Iceland"),
        oxigram_core::tr!("India"),
        oxigram_core::tr!("Indonesia"),
        oxigram_core::tr!("Iran"),
        oxigram_core::tr!("Iraq"),
        oxigram_core::tr!("Ireland"),
        oxigram_core::tr!("Isle of Man"),
        oxigram_core::tr!("Israel"),
        oxigram_core::tr!("Italy"),
        oxigram_core::tr!("Jamaica"),
        oxigram_core::tr!("Japan"),
        oxigram_core::tr!("Jersey"),
        oxigram_core::tr!("Jordan"),
        oxigram_core::tr!("Kenya"),
        oxigram_core::tr!("Kiribati"),
        oxigram_core::tr!("Kosovo"),
        oxigram_core::tr!("Kuwait"),
        oxigram_core::tr!("Kyrgyzstan"),
        oxigram_core::tr!("Laos"),
        oxigram_core::tr!("Latvia"),
        oxigram_core::tr!("Lebanon"),
        oxigram_core::tr!("Lesotho"),
        oxigram_core::tr!("Liberia"),
        oxigram_core::tr!("Libya"),
        oxigram_core::tr!("Liechtenstein"),
        oxigram_core::tr!("Lithuania"),
        oxigram_core::tr!("Luxembourg"),
        oxigram_core::tr!("Macao"),
        oxigram_core::tr!("Madagascar"),
        oxigram_core::tr!("Malawi"),
        oxigram_core::tr!("Malaysia"),
        oxigram_core::tr!("Maldives"),
        oxigram_core::tr!("Mali"),
        oxigram_core::tr!("Malta"),
        oxigram_core::tr!("Marshall Islands"),
        oxigram_core::tr!("Martinique"),
        oxigram_core::tr!("Mauritania"),
        oxigram_core::tr!("Mauritius"),
        oxigram_core::tr!("Mayotte"),
        oxigram_core::tr!("Mexico"),
        oxigram_core::tr!("Micronesia"),
        oxigram_core::tr!("Moldova"),
        oxigram_core::tr!("Monaco"),
        oxigram_core::tr!("Mongolia"),
        oxigram_core::tr!("Montenegro"),
        oxigram_core::tr!("Montserrat"),
        oxigram_core::tr!("Morocco"),
        oxigram_core::tr!("Mozambique"),
        oxigram_core::tr!("Myanmar"),
        oxigram_core::tr!("Namibia"),
        oxigram_core::tr!("Nauru"),
        oxigram_core::tr!("Nepal"),
        oxigram_core::tr!("Netherlands"),
        oxigram_core::tr!("New Caledonia"),
        oxigram_core::tr!("New Zealand"),
        oxigram_core::tr!("Nicaragua"),
        oxigram_core::tr!("Niger"),
        oxigram_core::tr!("Nigeria"),
        oxigram_core::tr!("Niue"),
        oxigram_core::tr!("Norfolk Island"),
        oxigram_core::tr!("North Korea"),
        oxigram_core::tr!("North Macedonia"),
        oxigram_core::tr!("Northern Mariana Islands"),
        oxigram_core::tr!("Norway"),
        oxigram_core::tr!("Svalbard & Jan Mayen"),
        oxigram_core::tr!("Oman"),
        oxigram_core::tr!("Pakistan"),
        oxigram_core::tr!("Palau"),
        oxigram_core::tr!("Palestine"),
        oxigram_core::tr!("Panama"),
        oxigram_core::tr!("Papua New Guinea"),
        oxigram_core::tr!("Paraguay"),
        oxigram_core::tr!("Peru"),
        oxigram_core::tr!("Philippines"),
        oxigram_core::tr!("Poland"),
        oxigram_core::tr!("Portugal"),
        oxigram_core::tr!("Puerto Rico"),
        oxigram_core::tr!("Qatar"),
        oxigram_core::tr!("Réunion"),
        oxigram_core::tr!("Romania"),
        oxigram_core::tr!("Rwanda"),
        oxigram_core::tr!("Samoa"),
        oxigram_core::tr!("San Marino"),
        oxigram_core::tr!("São Tomé & Príncipe"),
        oxigram_core::tr!("Saudi Arabia"),
        oxigram_core::tr!("Senegal"),
        oxigram_core::tr!("Serbia"),
        oxigram_core::tr!("Seychelles"),
        oxigram_core::tr!("Sierra Leone"),
        oxigram_core::tr!("Singapore"),
        oxigram_core::tr!("Sint Maarten"),
        oxigram_core::tr!("Slovakia"),
        oxigram_core::tr!("Slovenia"),
        oxigram_core::tr!("Solomon Islands"),
        oxigram_core::tr!("Somalia"),
        oxigram_core::tr!("South Africa"),
        oxigram_core::tr!("South Korea"),
        oxigram_core::tr!("South Sudan"),
        oxigram_core::tr!("Spain"),
        oxigram_core::tr!("Sri Lanka"),
        oxigram_core::tr!("St. Barthélemy"),
        oxigram_core::tr!("St. Helena"),
        oxigram_core::tr!("St. Kitts & Nevis"),
        oxigram_core::tr!("St. Lucia"),
        oxigram_core::tr!("St. Martin"),
        oxigram_core::tr!("St. Pierre & Miquelon"),
        oxigram_core::tr!("St. Vincent & Grenadines"),
        oxigram_core::tr!("Sudan"),
        oxigram_core::tr!("Suriname"),
        oxigram_core::tr!("Sweden"),
        oxigram_core::tr!("Switzerland"),
        oxigram_core::tr!("Syria"),
        oxigram_core::tr!("Taiwan"),
        oxigram_core::tr!("Tajikistan"),
        oxigram_core::tr!("Tanzania"),
        oxigram_core::tr!("Thailand"),
        oxigram_core::tr!("Timor-Leste"),
        oxigram_core::tr!("Togo"),
        oxigram_core::tr!("Tokelau"),
        oxigram_core::tr!("Tonga"),
        oxigram_core::tr!("Trinidad & Tobago"),
        oxigram_core::tr!("Tunisia"),
        oxigram_core::tr!("Turkey"),
        oxigram_core::tr!("Turkmenistan"),
        oxigram_core::tr!("Turks & Caicos Islands"),
        oxigram_core::tr!("Tuvalu"),
        oxigram_core::tr!("U.S. Virgin Islands"),
        oxigram_core::tr!("Uganda"),
        oxigram_core::tr!("United Arab Emirates"),
        oxigram_core::tr!("Uruguay"),
        oxigram_core::tr!("Uzbekistan"),
        oxigram_core::tr!("Vanuatu"),
        oxigram_core::tr!("Vatican City"),
        oxigram_core::tr!("Venezuela"),
        oxigram_core::tr!("Vietnam"),
        oxigram_core::tr!("Wallis & Futuna"),
        oxigram_core::tr!("Western Sahara"),
        oxigram_core::tr!("Yemen"),
        oxigram_core::tr!("Zambia"),
        oxigram_core::tr!("Zimbabwe"),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        assert_eq!(from_locale("ru-RU").unwrap().iso, "RU");
        assert_eq!(from_locale("uk-UA").unwrap().iso, "UA");
        assert_eq!(from_locale("en-US").unwrap().iso, "US");
        assert_eq!(from_locale("be-BY").unwrap().iso, "BY");
        assert_eq!(from_locale("kk-KZ").unwrap().iso, "KZ");
        assert_eq!(from_locale("de_DE").unwrap().iso, "DE");
        assert_eq!(from_locale("ru").unwrap().iso, "RU");
        assert_eq!(from_locale("uk").unwrap().iso, "UA");
        assert_eq!(from_locale("be").unwrap().iso, "BY");
        assert_eq!(from_locale("kk").unwrap().iso, "KZ");
        assert!(from_locale("en").is_none());
        assert!(from_locale("").is_none());
    }

    #[test]
    fn splitting() {
        let (c, r) = split("+7 (916) 123-45-67").unwrap();
        assert_eq!((c.iso, r.as_str()), ("RU", "9161234567"));
        let (c, r) = split("+380 50 123 4567").unwrap();
        assert_eq!((c.iso, r.as_str()), ("UA", "501234567"));
        let (c, r) = split("+1 650 555 1212").unwrap();
        assert_eq!((c.iso, r.as_str()), ("US", "6505551212"));
        let (c, r) = split("+77012345678").unwrap();
        assert_eq!((c.iso, r.as_str()), ("KZ", "7012345678"));
        assert_eq!(split("+7").unwrap().0.iso, "RU");
        assert!(split("+").is_none());
    }

    #[test]
    fn formatting() {
        let ru = by_iso("ru").unwrap();
        assert_eq!(format(ru, "9161234"), "916 123 4");
        assert_eq!(format(ru, ""), "");
        assert_eq!(format(ru, "91612345678"), "916 123 45 678");
        assert_eq!(format(by_iso("US").unwrap(), "6505551212"), "650 555 1212");
    }

    #[test]
    fn searching() {
        let id = |c: &'static Country| c.name.to_string();
        assert!(search("+38", id).iter().any(|c| c.iso == "UA"));
        assert!(search("+38", id).iter().all(|c| c.dial.starts_with("38")));
        assert!(search("380", id).iter().any(|c| c.iso == "UA"));
        assert_eq!(search("germ", id)[0].iso, "DE");
        assert_eq!(search("de", id).iter().any(|c| c.iso == "DE"), true);
        assert_eq!(search("", id).len(), all().len());
        let ru = |c: &'static Country| if c.iso == "DE" { "Германия".into() } else { c.name.to_string() };
        assert_eq!(search("герм", ru).len(), 1);
    }

    #[test]
    fn table_sane() {
        assert!(all().len() >= 230);
        let mut seen = std::collections::HashSet::new();
        for c in all() {
            assert!(seen.insert(c.iso), "dup {}", c.iso);
            assert!(c.dial.chars().all(|x| x.is_ascii_digit()) && !c.dial.is_empty());
            assert!(!c.mask.is_empty() && c.mask.chars().all(|x| x == '0' || x == ' '));
        }
        assert_eq!(flag("ua"), "\u{1F1FA}\u{1F1E6}");
    }
}
