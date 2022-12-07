pub mod de {
    use serde::de::Deserialize as _;
    use serde::de::Error as _;
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    pub fn rfc3339<'de, D>(d: D) -> Result<OffsetDateTime, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        OffsetDateTime::parse(&String::deserialize(d)?, &Rfc3339).map_err(D::Error::custom)
    }
}
