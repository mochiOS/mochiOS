"""Read only the named X11 window; avoid process startup and PNG defaults."""
import ctypes as c
from PIL import Image

class Attributes(c.Structure):
    _fields_ = [('x', c.c_int), ('y', c.c_int), ('width', c.c_int), ('height', c.c_int), ('border_width', c.c_int), ('depth', c.c_int), ('visual', c.c_void_p), ('root', c.c_ulong), ('window_class', c.c_int), ('bit_gravity', c.c_int), ('win_gravity', c.c_int), ('backing_store', c.c_int), ('backing_planes', c.c_ulong), ('backing_pixel', c.c_ulong), ('save_under', c.c_int), ('colormap', c.c_ulong), ('map_installed', c.c_int), ('map_state', c.c_int), ('all_event_masks', c.c_long), ('your_event_mask', c.c_long), ('do_not_propagate_mask', c.c_long), ('override_redirect', c.c_int), ('screen', c.c_void_p)]

class XImage(c.Structure):
    _fields_ = [('width', c.c_int), ('height', c.c_int), ('xoffset', c.c_int), ('format', c.c_int), ('data', c.c_void_p), ('byte_order', c.c_int), ('bitmap_unit', c.c_int), ('bitmap_bit_order', c.c_int), ('bitmap_pad', c.c_int), ('depth', c.c_int), ('bytes_per_line', c.c_int), ('bits_per_pixel', c.c_int), ('red_mask', c.c_ulong), ('green_mask', c.c_ulong), ('blue_mask', c.c_ulong), ('obdata', c.c_void_p)]

def capture(window, path):
    lib = c.CDLL('libX11.so.6')
    lib.XOpenDisplay.argtypes = [c.c_char_p]
    lib.XOpenDisplay.restype = c.c_void_p
    lib.XGetWindowAttributes.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(Attributes)]
    lib.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_ulong, c.c_int]
    lib.XGetImage.restype = c.POINTER(XImage)
    lib.XDestroyImage.argtypes = [c.POINTER(XImage)]
    lib.XCloseDisplay.argtypes = [c.c_void_p]
    display = lib.XOpenDisplay(None)
    if not display:
        raise RuntimeError('X11 display unavailable')
    image = None
    try:
        a = Attributes()
        if not lib.XGetWindowAttributes(display, window, c.byref(a)) or not (0 < a.width <= 8192 and 0 < a.height <= 8192):
            raise RuntimeError('Invalid QEMU window extent')
        image = lib.XGetImage(display, window, 0, 0, a.width, a.height, c.c_ulong(-1).value, 2)
        if not image:
            raise RuntimeError('XGetImage failed')
        x = image.contents
        if (x.bits_per_pixel, x.byte_order, x.red_mask, x.green_mask, x.blue_mask) != (32, 0, 16711680, 65280, 255) or not a.width * 4 <= x.bytes_per_line <= 8192 * 4:
            raise RuntimeError('Unsupported X11 pixel format')
        data = c.string_at(x.data, x.bytes_per_line * a.height)
        result = Image.frombytes('RGB', (a.width, a.height), data, 'raw', 'BGRX', x.bytes_per_line)
        result.save(path, compress_level=1)
    finally:
        if image:
            lib.XDestroyImage(image)
        lib.XCloseDisplay(display)

def resize(window, width=1280, height=825):
    lib = c.CDLL('libX11.so.6')
    lib.XOpenDisplay.argtypes = [c.c_char_p]
    lib.XOpenDisplay.restype = c.c_void_p
    lib.XResizeWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_uint, c.c_uint]
    lib.XFlush.argtypes = [c.c_void_p]
    lib.XCloseDisplay.argtypes = [c.c_void_p]
    display = lib.XOpenDisplay(None)
    if not display:
        raise RuntimeError('X11 display unavailable')
    try:
        lib.XResizeWindow(display, window, width, height)
        lib.XFlush(display)
    finally:
        lib.XCloseDisplay(display)
