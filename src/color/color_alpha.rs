use core::fmt;
use core::marker::PhantomData;
use core::ops::*;

use crate::{
    error::DowncastError, traits::*, ColAlpha, Color, ColorResult, Display, DynamicAlphaState,
    DynamicColor, DynamicColorSpace, DynamicState, EncodedSrgb, LinearSrgb, Premultiplied,
    Separate,
};

use glam::{Vec4, Vec4Swizzles};
#[cfg(all(not(feature = "std"), feature = "libm"))]
use num_traits::Float;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A strongly typed color with an alpha channel, parameterized by a color space and alpha state.
///
/// A color with an alpha channel is always in display-referred state. The alpha channel is always
/// linear [0..1].
///
/// See crate-level docs as well as [`ColorSpace`] and [`AlphaState`] for more.
#[repr(C)]
pub struct ColorAlpha<Spc, A> {
    /// The raw values of the color. Be careful when modifying this directly.
    pub raw: Vec4,
    _pd: PhantomData<(Spc, A)>,
}

impl<Spc, A> ColorAlpha<Spc, A> {
    /// Creates a [`ColorAlpha`] with the raw internal color elements `el1`, `el2`, `el3` and alpha value `alpha`.
    #[inline]
    pub fn new(el1: f32, el2: f32, el3: f32, alpha: f32) -> Self {
        Self::from_raw(Vec4::new(el1, el2, el3, alpha))
    }

    /// Creates a [`ColorAlpha`] with raw values contained in `raw`.
    #[inline]
    pub const fn from_raw(raw: Vec4) -> Self {
        Self {
            raw,
            _pd: PhantomData,
        }
    }

    /// Clamp the raw element values of `self` in the range [0..1]
    #[inline]
    pub fn saturate(self) -> Self {
        Self::from_raw(self.raw.min(Vec4::ONE).max(Vec4::ZERO))
    }

    /// Get the maximum element of `self`
    pub fn max_element(self) -> f32 {
        self.raw.max_element()
    }

    /// Get the minimum element of `self`
    pub fn min_element(self) -> f32 {
        self.raw.min_element()
    }
}

/// Creates a [`ColorAlpha`] in the [`EncodedSrgb`] color space with components `r`, `g`, `b`, and `a`.
#[inline]
pub fn srgba<A: AlphaState>(r: f32, g: f32, b: f32, a: f32) -> ColorAlpha<EncodedSrgb, A> {
    ColorAlpha::new(r, g, b, a)
}

/// Creates a [`ColorAlpha`] in the [`EncodedSrgb`] color space with components `r`, `g`, `b`, and `a`.
#[inline]
pub fn srgba_u8<A: AlphaState>(r: u8, g: u8, b: u8, a: u8) -> ColorAlpha<EncodedSrgb, A> {
    ColorAlpha::from_u8([r, g, b, a])
}

/// Creates a [`ColorAlpha`] in the [`LinearSrgb`] color space with components `r`, `g`, `b`, and `a`
#[inline]
pub fn linear_srgba<A: AlphaState>(r: f32, g: f32, b: f32, a: f32) -> ColorAlpha<LinearSrgb, A> {
    ColorAlpha::new(r, g, b, a)
}

impl<SrcSpace, SrcAlpha> ColorAlpha<SrcSpace, SrcAlpha>
where
    SrcSpace: ColorSpace,
    SrcAlpha: AlphaState,
{
    /// Converts from one color space and state to another.
    ///
    /// * If converting from [Premultiplied] to [Separate], you must ensure that `self.alpha != 0.0`, otherwise
    /// a divide by 0 will occur and `Inf`s will result.
    pub fn convert<DstSpace, DstAlpha>(self) -> ColorAlpha<DstSpace, DstAlpha>
    where
        DstSpace: ConvertFromRaw<SrcSpace>,
        DstAlpha: AlphaState,
    {
        let alpha = self.raw.w;

        let linear = <DstSpace as ConvertFromRaw<SrcSpace>>::src_transform_raw(self.raw.xyz());

        let separate = <SrcAlpha as ConvertToAlphaRaw<Separate>>::convert_raw(linear, alpha);

        let dst_linear = <DstSpace as ConvertFromRaw<SrcSpace>>::linear_part_raw(separate);

        let dst_alpha = <DstAlpha as ConvertFromAlphaRaw<Separate>>::convert_raw(dst_linear, alpha);

        let dst = <DstSpace as ConvertFromRaw<SrcSpace>>::dst_transform_raw(dst_alpha);

        ColorAlpha::from_raw(dst.extend(alpha))
    }

    /// Converts from one color space and state to another.
    ///
    /// This works the same as [`convert`][Color::convert] except there is only one type parameter, the
    /// "[Query][ColorAlphaConversionQuery]".
    ///
    /// The query is meant to be one of:
    /// * A [`ColorSpace`]
    /// * A [`AlphaState`]
    /// * A [`ColorAlpha`] (in which case it will be converted to that color's space and alpha state)
    ///
    /// This query is slightly more generic than the ones on [`convert`][ColorAlpha::convert], which
    /// means that the Rust type system is usually not able to infer the query without you explicitly giving one.
    ///
    /// This can be useful in conjunction with defined type aliases for predefined [`ColorAlpha`] types.
    pub fn convert_to<Query>(self) -> ColorAlpha<Query::DstSpace, Query::DstAlpha>
    where
        Query: ColorAlphaConversionQuery<SrcSpace, SrcAlpha>,
    {
        self.convert::<Query::DstSpace, Query::DstAlpha>()
    }

    /// Converts `self` to the provided `DstAlpha` [`AlphaState`].
    ///
    /// * If converting to the same state, this is a no-op.
    /// * If converting from [Premultiplied] to [Separate], you must ensure that `self.alpha != 0.0`, otherwise
    /// a divide by 0 will occur and `Inf`s will result.
    pub fn convert_alpha<DstAlpha: ConvertFromAlphaRaw<SrcAlpha> + AlphaState>(
        self,
    ) -> ColorAlpha<SrcSpace, DstAlpha> {
        let raw = self.raw.xyz();
        let alpha = self.raw.w;
        let converted = <DstAlpha as ConvertFromAlphaRaw<SrcAlpha>>::convert_raw(raw, alpha);
        ColorAlpha::from_raw(converted.extend(alpha))
    }

    /// Interprets this color as `DstSpace`. This assumes you have done an external computation/conversion such that this
    /// cast is valid.
    pub fn cast_space<DstSpace: ColorSpace>(self) -> ColorAlpha<DstSpace, SrcAlpha> {
        ColorAlpha::from_raw(self.raw)
    }

    /// Changes this color's alpha state. This assumes that you have done some kind of computation/conversion such that this
    /// cast is valid.
    pub fn cast_alpha_state<DstAlpha: AlphaState>(self) -> ColorAlpha<SrcSpace, DstAlpha> {
        ColorAlpha::from_raw(self.raw)
    }

    /// Changes this color's alpha state. This assumes that you have done some kind of computation/conversion such that this
    /// cast is valid.
    pub fn cast<DstSpace: ColorSpace, DstAlpha: AlphaState>(
        self,
    ) -> ColorAlpha<DstSpace, DstAlpha> {
        ColorAlpha::from_raw(self.raw)
    }
}

impl<Spc: WorkingColorSpace> ColorAlpha<Spc, Separate> {
    /// Blend `self`'s color values with the color values from `other`. Does not blend alpha.
    pub fn blend<Blender: ColorBlender>(
        self,
        other: ColorAlpha<Spc, Separate>,
        factor: f32,
    ) -> ColorAlpha<Spc, Separate> {
        self.blend_with::<Blender>(other, factor, Default::default())
    }

    /// Blend `self`'s color values with the color values from `other`. Also blends alpha.
    pub fn blend_alpha<Blender: ColorBlender>(
        self,
        other: ColorAlpha<Spc, Separate>,
        factor: f32,
    ) -> ColorAlpha<Spc, Separate> {
        self.blend_alpha_with::<Blender>(other, factor, Default::default())
    }

    /// Blend `self`'s color values with the color values from `other`. Does not blend alpha.
    pub fn blend_with<Blender: ColorBlender>(
        self,
        other: ColorAlpha<Spc, Separate>,
        factor: f32,
        params: Blender::Params,
    ) -> ColorAlpha<Spc, Separate> {
        let raw1 = self.raw;
        let raw2 = other.raw;
        let x = Blender::blend_with(raw1.x, raw2.x, factor, params);
        let y = Blender::blend_with(raw1.y, raw2.y, factor, params);
        let z = Blender::blend_with(raw1.z, raw2.z, factor, params);
        ColorAlpha::from_raw(Vec4::new(x, y, z, raw1.w))
    }

    /// Blend `self`'s color values with the color values from `other`. Also blends alpha.
    pub fn blend_alpha_with<Blender: ColorBlender>(
        self,
        other: ColorAlpha<Spc, Separate>,
        factor: f32,
        params: Blender::Params,
    ) -> ColorAlpha<Spc, Separate> {
        let raw1 = self.raw;
        let raw2 = other.raw;
        let x = Blender::blend_with(raw1.x, raw2.x, factor, params);
        let y = Blender::blend_with(raw1.y, raw2.y, factor, params);
        let z = Blender::blend_with(raw1.z, raw2.z, factor, params);
        let a = Blender::blend_with(raw1.w, raw2.w, factor, params);
        ColorAlpha::from_raw(Vec4::new(x, y, z, a))
    }
}

impl<Spc: LinearColorSpace, A: AlphaState> ColorAlpha<Spc, A>
where
    Premultiplied: ConvertFromAlphaRaw<A>,
{
    /// Premultiplies `self` by multiplying its color components by its alpha. Does nothing if `self` is already premultiplied.
    pub fn premultiply(self) -> ColorAlpha<Spc, Premultiplied> {
        let raw = self.raw.xyz();
        let alpha = self.raw.w;
        let converted = <Premultiplied as ConvertFromAlphaRaw<A>>::convert_raw(raw, alpha);
        ColorAlpha::from_raw(converted.extend(alpha))
    }
}

impl<Spc: LinearColorSpace, A: AlphaState> ColorAlpha<Spc, A>
where
    Separate: ConvertFromAlphaRaw<A>,
{
    /// Separates `self` by dividing its color components by its alpha. Does nothing if `self` is already separate.
    ///
    /// * You must ensure that `self.alpha != 0.0`, otherwise
    /// a divide by 0 will occur and `Inf`s will result.
    pub fn separate(self) -> ColorAlpha<Spc, Separate> {
        let raw = self.raw.xyz();
        let alpha = self.raw.w;
        let converted = <Separate as ConvertFromAlphaRaw<A>>::convert_raw(raw, alpha);
        ColorAlpha::from_raw(converted.extend(alpha))
    }
}

impl<Spc: NonlinearColorSpace, A: AlphaState> ColorAlpha<Spc, A> {
    /// Convert `self` into the closest linear color space.
    pub fn linearize(self) -> ColorAlpha<Spc::LinearSpace, A> {
        use kolor::details::{color::TransformFn, transform::ColorTransform};
        let spc = Spc::SPACE;
        ColorAlpha::from_raw(
            ColorTransform::new(spc.transform_function(), TransformFn::NONE)
                .unwrap()
                .apply(self.raw.xyz(), spc.white_point())
                .extend(self.raw.w),
        )
    }
}

impl<SrcSpace: EncodedColorSpace, A: AlphaState> ColorAlpha<SrcSpace, A> {
    /// Decode `self` into its decoded ([working][WorkingColorSpace]) color space,
    /// which allows many more operations to be performed.
    pub fn decode(self) -> ColorAlpha<SrcSpace::DecodedSpace, A> {
        let raw_xyz =
            <SrcSpace::DecodedSpace as ConvertFromRaw<SrcSpace>>::src_transform_raw(self.raw.xyz());
        ColorAlpha::from_raw(raw_xyz.extend(self.raw.w))
    }
}

impl<Spc, A> From<ColorAlpha<Spc, A>> for Color<Spc, Display>
where
    Spc: ColorSpace,
    A: AlphaState,
    Premultiplied: ConvertFromAlphaRaw<A>,
{
    fn from(c: ColorAlpha<Spc, A>) -> Self {
        c.into_color()
    }
}

impl<Spc, A> ColorAlpha<Spc, A> {
    /// Converts `self` to a [`Color`] by stripping off the alpha component.
    pub fn into_color_no_premultiply(self) -> Color<Spc, Display> {
        Color::from_raw(self.raw.xyz())
    }
}

impl<Spc, A> ColorAlpha<Spc, A>
where
    Spc: ColorSpace,
    A: AlphaState,
    Premultiplied: ConvertFromAlphaRaw<A>,
{
    /// Converts `self` to a [`Color`] by first premultiplying `self` (if premultiplying makes sense for the current color space)
    /// and then stripping off the alpha component.
    pub fn into_color(self) -> Color<Spc, Display> {
        if Spc::SPACE != Spc::LinearSpace::SPACE {
            Color::from_raw(self.convert_alpha::<Premultiplied>().raw.xyz())
        } else {
            Color::from_raw(self.raw.xyz())
        }
    }
}

impl<Spc: AsU8Array, A: AlphaState> ColorAlpha<Spc, A> {
    /// Convert `self` to a `[u8; 4]`. All components of `self` *must* be in range `[0..1]`.
    pub fn to_u8(self) -> [u8; 4] {
        fn f32_to_u8(x: f32) -> u8 {
            (x * 255.0).round() as u8
        }
        [
            f32_to_u8(self.raw.x),
            f32_to_u8(self.raw.y),
            f32_to_u8(self.raw.z),
            f32_to_u8(self.raw.w),
        ]
    }

    /// Decode a `[u8; 4]` into a `ColorAlpha` with specified space and alpha state.
    pub fn from_u8(encoded: [u8; 4]) -> ColorAlpha<Spc, A> {
        fn u8_to_f32(x: u8) -> f32 {
            x as f32 / 255.0
        }
        ColorAlpha::new(
            u8_to_f32(encoded[0]),
            u8_to_f32(encoded[1]),
            u8_to_f32(encoded[2]),
            u8_to_f32(encoded[3]),
        )
    }
}

impl<SrcSpace, DstSpace, SrcAlpha, DstAlpha> ColorInto<ColorAlpha<DstSpace, DstAlpha>>
    for ColorAlpha<SrcSpace, SrcAlpha>
where
    DstSpace: ConvertFromRaw<SrcSpace>,
    SrcSpace: ColorSpace,
    DstAlpha: ConvertFromAlphaRaw<SrcAlpha> + AlphaState,
    SrcAlpha: AlphaState,
{
    fn into(self) -> ColorAlpha<DstSpace, DstAlpha> {
        self.convert()
    }
}

impl<Spc, A> fmt::Display for ColorAlpha<Spc, A>
where
    Spc: ColorSpace,
    A: AlphaState,
    ColorAlpha<Spc, A>: Deref<Target = ColAlpha<Spc::ComponentStruct>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ColorAlpha<{}, {}>: ({})",
            Spc::default(),
            A::default(),
            self.deref()
        )
    }
}

impl<Spc, A> fmt::Debug for ColorAlpha<Spc, A>
where
    Spc: ColorSpace,
    A: AlphaState,
    ColorAlpha<Spc, A>: Deref<Target = ColAlpha<Spc::ComponentStruct>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self)
    }
}

impl<Spc, A> Copy for ColorAlpha<Spc, A> {}

impl<Spc, A> Clone for ColorAlpha<Spc, A> {
    fn clone(&self) -> ColorAlpha<Spc, A> {
        *self
    }
}

impl<Spc, A> PartialEq for ColorAlpha<Spc, A> {
    fn eq(&self, other: &ColorAlpha<Spc, A>) -> bool {
        self.raw == other.raw
    }
}

unsafe impl<Spc, A> bytemuck::Zeroable for ColorAlpha<Spc, A> {}
unsafe impl<Spc, A> bytemuck::TransparentWrapper<Vec4> for ColorAlpha<Spc, A> {}
unsafe impl<Spc: 'static, A: 'static> bytemuck::Pod for ColorAlpha<Spc, A> {}

macro_rules! impl_op_color {
    ($op:ident, $op_func:ident) => {
        impl<Spc: LinearColorSpace, A> $op for ColorAlpha<Spc, A> {
            type Output = ColorAlpha<Spc, A>;
            fn $op_func(self, rhs: ColorAlpha<Spc, A>) -> Self::Output {
                ColorAlpha::from_raw(self.raw.$op_func(rhs.raw))
            }
        }
    };
}

macro_rules! impl_op_color_float {
    ($op:ident, $op_func:ident) => {
        impl<Spc: LinearColorSpace, A> $op<f32> for ColorAlpha<Spc, A> {
            type Output = ColorAlpha<Spc, A>;
            fn $op_func(self, rhs: f32) -> Self::Output {
                ColorAlpha::from_raw(self.raw.$op_func(rhs))
            }
        }

        impl<Spc: LinearColorSpace, A> $op<ColorAlpha<Spc, A>> for f32 {
            type Output = ColorAlpha<Spc, A>;
            fn $op_func(self, rhs: ColorAlpha<Spc, A>) -> Self::Output {
                ColorAlpha::from_raw(self.$op_func(rhs.raw))
            }
        }
    };
}

impl_op_color!(Add, add);
impl_op_color!(Sub, sub);
impl_op_color!(Mul, mul);
impl_op_color!(Div, div);

impl_op_color_float!(Mul, mul);
impl_op_color_float!(Div, div);

/// A dynamic color with an alpha channel, with its space and alpha defined
/// as data. This is mostly useful for (de)serialization.
///
/// See [`ColorAlpha`], [`ColorSpace`] and [`AlphaState`] for more.
#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "with-serde", derive(Serialize, Deserialize))]
pub struct DynamicColorAlpha {
    /// The raw tristimulus value of the color. Be careful when modifying this directly, i.e.
    /// don't multiply two Colors' raw values unless they are in the same color space and state.
    pub raw: Vec4,
    pub space: DynamicColorSpace,
    pub alpha_state: DynamicAlphaState,
}

impl DynamicColorAlpha {
    /// Create a new [`DynamicColorAlpha`] with specified raw color components, color space, and alpha state.
    pub fn new(raw: Vec4, space: DynamicColorSpace, alpha_state: DynamicAlphaState) -> Self {
        Self {
            raw,
            space,
            alpha_state,
        }
    }

    /// Converts `self` to a [`DynamicColor`] by first premultiplying `self` if it is not already
    /// and then stripping off the alpha component.
    pub fn into_color(self) -> DynamicColor {
        let color_alpha = self.convert_alpha_state(DynamicAlphaState::Premultiplied);
        DynamicColor::new(color_alpha.raw.xyz(), self.space, DynamicState::Display)
    }

    /// Converts `self` to a [`DynamicColor`] by stripping off the alpha component, without checking
    /// whether it is premultiplied or not.
    pub fn into_color_no_premultiply(self) -> DynamicColor {
        DynamicColor::new(self.raw.xyz(), self.space, DynamicState::Display)
    }

    /// Converts from one color space and state to another.
    ///
    /// * If converting from [Premultiplied][DynamicAlphaState::Premultiplied] to [Separate][DynamicAlphaState::Separate], if
    /// `self`'s alpha is 0.0, the resulting color values will not be changed.
    pub fn convert(mut self, dst_space: DynamicColorSpace, dst_alpha: DynamicAlphaState) -> Self {
        let conversion = kolor::ColorConversion::new(self.space, dst_space);

        // linearize
        self.raw = conversion.apply_src_transform(self.raw.xyz()).extend(1.0);

        // separate
        self = self.convert_alpha_state(DynamicAlphaState::Separate);

        // linear color conversion
        self.raw = conversion.apply_linear_part(self.raw.xyz()).extend(1.0);

        // convert to dst alpha state
        self = self.convert_alpha_state(dst_alpha);

        // dst transform
        self.raw = conversion.apply_dst_transform(self.raw.xyz()).extend(1.0);
        self.space = dst_space;

        self
    }

    /// Convert `self` to the specified space and downcast it to a typed [`ColorAlpha`] with the space
    /// and state specified.
    pub fn downcast_convert<DstSpace, DstAlpha>(self) -> ColorAlpha<DstSpace, DstAlpha>
    where
        DstSpace: ColorSpace,
        DstAlpha: AlphaState,
    {
        let dst = self.convert(DstSpace::SPACE, DstAlpha::STATE);
        ColorAlpha::from_raw(dst.raw)
    }

    /// Converts `self` to the provided `dst_alpha` [`DynamicAlphaState`].
    ///
    /// * If converting to the same state, this is a no-op.
    /// * If converting from [Premultiplied][DynamicAlphaState::Premultiplied] to [Separate][DynamicAlphaState::Separate], if
    /// `self`'s alpha is 0.0, the resulting color values will not be changed.
    pub fn convert_alpha_state(self, dst_alpha: DynamicAlphaState) -> DynamicColorAlpha {
        let col = match (self.alpha_state, dst_alpha) {
            (DynamicAlphaState::Separate, DynamicAlphaState::Premultiplied) => {
                self.raw.xyz() * self.raw.w
            }
            (DynamicAlphaState::Premultiplied, DynamicAlphaState::Separate) => {
                if self.raw.w != 0.0 {
                    self.raw.xyz() / self.raw.w
                } else {
                    self.raw.xyz()
                }
            }
            _ => self.raw.xyz(),
        };

        Self {
            raw: col.extend(self.raw.w),
            space: self.space,
            alpha_state: dst_alpha,
        }
    }
}

impl<'a> From<&'a dyn AnyColorAlpha> for DynamicColorAlpha {
    fn from(color: &'a dyn AnyColorAlpha) -> DynamicColorAlpha {
        color.dynamic()
    }
}

impl<C: AnyColorAlpha> DynColorAlpha for C {
    /// Attempt to convert to a typed [`ColorAlpha`]. Returns an error if `self`'s color space and alpha state do not match
    /// the given types.
    fn downcast<Spc: ColorSpace, A: AlphaState>(&self) -> ColorResult<ColorAlpha<Spc, A>> {
        if self.space() != Spc::SPACE {
            return Err(DowncastError::MismatchedSpace(self.space(), Spc::SPACE).into());
        }

        if self.alpha_state() != A::STATE {
            return Err(DowncastError::MismatchedAlphaState(self.alpha_state(), A::STATE).into());
        }

        Ok(ColorAlpha::from_raw(self.raw()))
    }

    /// Convert to a typed `ColorAlpha` without checking if the color space and state types
    /// match this color's space and state. Use only if you are sure that this color
    /// is in the correct format.
    fn downcast_unchecked<Spc: ColorSpace, A: AlphaState>(&self) -> ColorAlpha<Spc, A> {
        ColorAlpha::from_raw(self.raw())
    }
}
