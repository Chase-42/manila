use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use std::fmt;

// Integer cents - the only monetary unit in this codebase.
// Uses #[serde(transparent)] so JSON serializes as a plain number.
// No #[derive(TS)] - IPC struct fields annotate with #[ts(type = "number")] instead,
// which keeps the generated TypeScript files identical to their pre-migration form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cents(pub i64);

impl Cents {
    pub fn zero() -> Self {
        Cents(0)
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Cents)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Cents)
    }

    pub fn abs(self) -> Self {
        Cents(self.0.abs())
    }
}

impl std::ops::Add for Cents {
    type Output = Cents;
    fn add(self, rhs: Cents) -> Cents {
        self.checked_add(rhs)
            .unwrap_or_else(|| panic!("Cents addition overflow"))
    }
}

impl std::ops::Sub for Cents {
    type Output = Cents;
    fn sub(self, rhs: Cents) -> Cents {
        self.checked_sub(rhs)
            .unwrap_or_else(|| panic!("Cents subtraction overflow"))
    }
}

impl std::ops::Neg for Cents {
    type Output = Cents;
    fn neg(self) -> Cents {
        Cents(-self.0)
    }
}

impl std::ops::Div<i64> for Cents {
    type Output = Cents;
    fn div(self, rhs: i64) -> Cents {
        Cents(self.0 / rhs)
    }
}

impl std::iter::Sum for Cents {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Cents::zero(), |a, b| a + b)
    }
}

impl fmt::Display for Cents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for Cents {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for Cents {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i64::column_result(value).map(Cents)
    }
}

// Entity ID newtypes.
// Same pattern: #[serde(transparent)] for JSON, no TS derive.
// IPC struct fields use #[ts(type = "string")] to keep generated TypeScript identical.
macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl From<String> for $name {
            fn from(s: String) -> Self {
                $name(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                $name(s.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                &self.0 == other
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                self.0.to_sql()
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                String::column_result(value).map($name)
            }
        }
    };
}

string_id!(AccountId);
string_id!(CategoryId);
string_id!(TransactionId);
string_id!(GoalId);
string_id!(GroupId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_checked_add_returns_none_on_overflow() {
        assert_eq!(Cents(i64::MAX).checked_add(Cents(1)), None);
        assert_eq!(Cents(100).checked_add(Cents(200)), Some(Cents(300)));
    }

    #[test]
    fn cents_checked_sub_returns_none_on_overflow() {
        assert_eq!(Cents(i64::MIN).checked_sub(Cents(1)), None);
        assert_eq!(Cents(200).checked_sub(Cents(100)), Some(Cents(100)));
    }

    #[test]
    fn id_newtypes_round_trip() {
        let tx = TransactionId::from("abc");
        assert_eq!(tx.as_ref(), "abc");
        let goal = GoalId::from("xyz");
        assert_eq!(goal.as_ref(), "xyz");
    }
}
