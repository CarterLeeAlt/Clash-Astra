// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, io, iter::once, mem, os::windows::ffi::OsStrExt, path::Path, sync::Arc};

use windows::{
  core::PCWSTR,
  Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
    System::LibraryLoader::*,
    UI::WindowsAndMessaging::*,
  },
};

use crate::{dpi::PhysicalSize, icon::*};

impl Pixel {
  fn to_bgra(&mut self) {
    mem::swap(&mut self.r, &mut self.b);
  }
}

/// Alpha below this is treated as transparent in the AND mask. Renderers that honor
/// the alpha channel ignore the mask; the mask only matters for mask-based drawing
/// (e.g. image lists with `ILC_MASK`), where a 50% cut gives the cleanest edge.
const AND_MASK_ALPHA_THRESHOLD: u8 = 128;

/// Builds the AND mask expected by `CreateIcon`: a monochrome bitmap with
/// 1 bit per pixel, MSB first, each scanline padded to a WORD (2-byte) boundary;
/// a set bit marks a transparent pixel.
///
/// Clash Astra patch: upstream tao 0.16.7 passed one byte per pixel instead, which
/// Windows reads as 1bpp rows, producing a mask with vertical transparent stripes
/// (visible as a "crack" through tray icons in Task Manager).
fn and_mask_from_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
  let stride = (width + 15) / 16 * 2;
  let mut mask = vec![0u8; stride * height];
  for y in 0..height {
    for x in 0..width {
      if rgba[(y * width + x) * PIXEL_SIZE + 3] < AND_MASK_ALPHA_THRESHOLD {
        mask[y * stride + x / 8] |= 0x80 >> (x % 8);
      }
    }
  }
  mask
}

impl RgbaIcon {
  fn into_windows_icon(self) -> Result<WinIcon, BadIcon> {
    let mut rgba = self.rgba;
    let pixel_count = rgba.len() / PIXEL_SIZE;
    let and_mask = and_mask_from_rgba(&rgba, self.width as usize, self.height as usize);
    let pixels =
      unsafe { std::slice::from_raw_parts_mut(rgba.as_mut_ptr() as *mut Pixel, pixel_count) };
    for pixel in pixels {
      pixel.to_bgra();
    }
    let handle = unsafe {
      CreateIcon(
        HINSTANCE::default(),
        self.width as i32,
        self.height as i32,
        1,
        (PIXEL_SIZE * 8) as u8,
        and_mask.as_ptr() as *const u8,
        rgba.as_ptr() as *const u8,
      )
    };
    Ok(WinIcon::from_handle(
      handle.map_err(|_| BadIcon::OsError(io::Error::last_os_error()))?,
    ))
  }
}

#[non_exhaustive]
#[derive(Debug)]
pub enum IconType {
  Small = ICON_SMALL as isize,
  Big = ICON_BIG as isize,
}

#[derive(Debug)]
struct RaiiIcon {
  handle: HICON,
}

#[derive(Clone)]
pub struct WinIcon {
  inner: Arc<RaiiIcon>,
}

unsafe impl Send for WinIcon {}

impl WinIcon {
  pub fn as_raw_handle(&self) -> HICON {
    self.inner.handle
  }

  pub fn from_path<P: AsRef<Path>>(
    path: P,
    size: Option<PhysicalSize<u32>>,
  ) -> Result<Self, BadIcon> {
    let wide_path: Vec<u16> = path
      .as_ref()
      .as_os_str()
      .encode_wide()
      .chain(once(0))
      .collect();

    // width / height of 0 along with LR_DEFAULTSIZE tells windows to load the default icon size
    let (width, height) = size.map(Into::into).unwrap_or((0, 0));

    let handle = unsafe {
      LoadImageW(
        HINSTANCE::default(),
        PCWSTR::from_raw(wide_path.as_ptr()),
        IMAGE_ICON,
        width as i32,
        height as i32,
        LR_DEFAULTSIZE | LR_LOADFROMFILE,
      )
    }
    .map(|handle| HICON(handle.0));
    Ok(WinIcon::from_handle(
      handle.map_err(|_| BadIcon::OsError(io::Error::last_os_error()))?,
    ))
  }

  pub fn from_resource(resource_id: u16, size: Option<PhysicalSize<u32>>) -> Result<Self, BadIcon> {
    // width / height of 0 along with LR_DEFAULTSIZE tells windows to load the default icon size
    let (width, height) = size.map(Into::into).unwrap_or((0, 0));
    let handle = unsafe {
      LoadImageW(
        GetModuleHandleW(PCWSTR::null()).unwrap_or_default(),
        PCWSTR::from_raw(resource_id as usize as *const u16),
        IMAGE_ICON,
        width as i32,
        height as i32,
        LR_DEFAULTSIZE,
      )
    }
    .map(|handle| HICON(handle.0));
    Ok(WinIcon::from_handle(
      handle.map_err(|_| BadIcon::OsError(io::Error::last_os_error()))?,
    ))
  }

  pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, BadIcon> {
    let rgba_icon = RgbaIcon::from_rgba(rgba, width, height)?;
    rgba_icon.into_windows_icon()
  }

  pub fn set_for_window(&self, hwnd: HWND, icon_type: IconType) {
    unsafe {
      SendMessageW(
        hwnd,
        WM_SETICON,
        WPARAM(icon_type as _),
        LPARAM(self.as_raw_handle().0),
      );
    }
  }

  fn from_handle(handle: HICON) -> Self {
    Self {
      inner: Arc::new(RaiiIcon { handle }),
    }
  }
}

impl Drop for RaiiIcon {
  fn drop(&mut self) {
    unsafe { DestroyIcon(self.handle) };
  }
}

impl fmt::Debug for WinIcon {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
    (*self.inner).fmt(formatter)
  }
}

pub fn unset_for_window(hwnd: HWND, icon_type: IconType) {
  unsafe {
    SendMessageW(hwnd, WM_SETICON, WPARAM(icon_type as _), LPARAM(0));
  }
}
