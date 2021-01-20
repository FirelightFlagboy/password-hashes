//! Implementation of the `password-hash` crate API.

use crate::{pbkdf2, simple};
use core::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};
use hmac::Hmac;
use password_hash::{
    errors::ParamsError, HasherError, Ident, McfHasher, Output, ParamsString, PasswordHash,
    PasswordHasher, Salt,
};
use sha2::{Sha256, Sha512};

#[cfg(feature = "sha1")]
use sha1::Sha1;

/// PBKDF2 (SHA-1)
#[cfg(feature = "sha1")]
pub const PBKDF2_SHA1: Ident = Ident::new("pbkdf2");

/// PBKDF2 (SHA-256)
pub const PBKDF2_SHA256: Ident = Ident::new("pbkdf2-sha256");

/// PBKDF2 (SHA-512)
pub const PBKDF2_SHA512: Ident = Ident::new("pbkdf2-sha512");

/// PBKDF2 type for use with [`PasswordHasher`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[cfg_attr(docsrs, doc(cfg(feature = "include_simple")))]
pub struct Pbkdf2;

impl PasswordHasher for Pbkdf2 {
    type Params = Params;

    fn hash_password<'a>(
        &self,
        password: &[u8],
        algorithm: Option<Ident<'a>>,
        params: Params,
        salt: Salt<'a>,
    ) -> Result<PasswordHash<'a>, HasherError> {
        let algorithm = AlgorithmId::new(algorithm.unwrap_or(PBKDF2_SHA256))?;

        let mut salt_arr = [0u8; 64];
        let salt_bytes = salt.b64_decode(&mut salt_arr)?;

        let output = Output::init_with(params.output_length, |out| {
            let f = match algorithm {
                #[cfg(feature = "sha1")]
                AlgorithmId::Sha1 => pbkdf2::<Hmac<Sha1>>,
                AlgorithmId::Sha256 => pbkdf2::<Hmac<Sha256>>,
                AlgorithmId::Sha512 => pbkdf2::<Hmac<Sha512>>,
            };

            f(password, salt_bytes, params.rounds, out);
            Ok(())
        })?;

        Ok(PasswordHash {
            algorithm: algorithm.ident(),
            version: None,
            params: params.try_into()?,
            salt: Some(salt),
            hash: Some(output),
        })
    }
}

impl McfHasher for Pbkdf2 {
    fn upgrade_mcf_hash<'a>(&self, hash: &'a str) -> Result<PasswordHash<'a>, HasherError> {
        use password_hash::errors::ParseError;

        // TODO(tarcieri): better error here?
        let (rounds, salt, hash) = simple::parse_hash(hash)
            .map_err(|_| HasherError::Parse(ParseError::InvalidChar('?')))?;

        let salt = Salt::new(b64_strip(salt))?;
        let hash = Output::b64_decode(b64_strip(hash))?;

        let params = Params {
            rounds,
            output_length: hash.len(),
        };

        Ok(PasswordHash {
            algorithm: PBKDF2_SHA256,
            version: None,
            params: params.try_into()?,
            salt: Some(salt),
            hash: Some(hash),
        })
    }
}

/// Strip trailing `=` signs off a Base64 value to make a valid B64 value
pub fn b64_strip(mut s: &str) -> &str {
    while s.ends_with('=') {
        s = &s[..(s.len() - 1)]
    }
    s
}

/// PBKDF2 variants.
///
/// <https://en.wikipedia.org/wiki/PBKDF2>
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[non_exhaustive]
#[cfg_attr(docsrs, doc(cfg(feature = "include_simple")))]
pub enum AlgorithmId {
    /// PBKDF2 SHA1
    #[cfg(feature = "sha1")]
    Sha1,

    /// PBKDF2 SHA-256
    Sha256,

    /// PBKDF2 SHA-512
    Sha512,
}

impl AlgorithmId {
    /// Parse an [`AlgorithmId`] from the provided [`Ident`]
    pub fn new(id: Ident<'_>) -> Result<Self, HasherError> {
        match id {
            #[cfg(feature = "sha1")]
            PBKDF2_SHA1 => Ok(AlgorithmId::Sha1),
            PBKDF2_SHA256 => Ok(AlgorithmId::Sha256),
            PBKDF2_SHA512 => Ok(AlgorithmId::Sha512),
            _ => Err(HasherError::Algorithm),
        }
    }

    /// Get the [`Ident`] that corresponds to this PBKDF2 [`AlgorithmId`].
    pub fn ident(&self) -> Ident<'static> {
        match self {
            #[cfg(feature = "sha1")]
            AlgorithmId::Sha1 => PBKDF2_SHA1,
            AlgorithmId::Sha256 => PBKDF2_SHA256,
            AlgorithmId::Sha512 => PBKDF2_SHA512,
        }
    }

    /// Get the identifier string for this PBKDF2 [`AlgorithmId`].
    pub fn as_str(&self) -> &str {
        self.ident().as_str()
    }
}

impl FromStr for AlgorithmId {
    type Err = HasherError;

    fn from_str(s: &str) -> Result<AlgorithmId, HasherError> {
        Self::new(Ident::try_from(s)?)
    }
}

impl AsRef<str> for AlgorithmId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<AlgorithmId> for Ident<'static> {
    fn from(alg: AlgorithmId) -> Ident<'static> {
        alg.ident()
    }
}

impl fmt::Display for AlgorithmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// PBKDF2 params
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[cfg_attr(docsrs, doc(cfg(feature = "include_simple")))]
pub struct Params {
    /// Number of rounds
    pub rounds: u32,

    /// Size of the output (in bytes)
    pub output_length: usize,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            rounds: 10_000,
            output_length: 32,
        }
    }
}

impl TryFrom<&ParamsString> for Params {
    type Error = HasherError;

    fn try_from(input: &ParamsString) -> Result<Self, HasherError> {
        let mut output = Params::default();

        for (ident, value) in input.iter() {
            match ident.as_str() {
                "i" => output.rounds = value.decimal()?,
                "l" => {
                    output.output_length = value
                        .decimal()?
                        .try_into()
                        .map_err(|_| ParamsError::InvalidValue)?
                }
                _ => return Err(ParamsError::InvalidName.into()),
            }
        }

        Ok(output)
    }
}

impl<'a> TryFrom<Params> for ParamsString {
    type Error = HasherError;

    fn try_from(input: Params) -> Result<ParamsString, HasherError> {
        let mut output = ParamsString::new();
        output.add_decimal("i", input.rounds)?;
        output.add_decimal("l", input.output_length as u32)?;
        Ok(output)
    }
}