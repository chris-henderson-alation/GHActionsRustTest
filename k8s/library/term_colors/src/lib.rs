//! term_colors is a collection of convenience functions for coloring terminal output.

use ansi_term::ANSIGenericString;
use std::borrow::Cow;

pub fn bold<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Style::new().bold().paint(input)
}

pub fn cyan<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::Cyan.paint(input)
}

pub fn red<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::Red.paint(input)
}

pub fn green<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::Green.paint(input)
}

pub fn blue<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::Blue.paint(input)
}

pub fn purple<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::Purple.paint(input)
}

pub fn orange<'a, I, S: 'a + ToOwned + ?Sized>(input: I) -> ANSIGenericString<'a, S>
where
    I: Into<Cow<'a, S>>,
    <S as ToOwned>::Owned: std::fmt::Debug,
{
    ansi_term::Color::RGB(243, 113, 33).paint(input)
}
