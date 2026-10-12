//! Rivet Canvas: render 2D por GPU (OpenGL 3.3 Core + EGL).
//!
//! Shaders, geometría, texturas y efectos (blur por FBO ping-pong).
//! Escribe en el buffer que `rivet-window` presenta; **no** depende de
//! `rivet-window` (frontera fija: window = protocolo, canvas = render).
