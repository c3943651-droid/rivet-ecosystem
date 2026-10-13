//! Compilación y caché de programas GL (GLSL 330 core).
//!
//! `docs/rivet-canvas.md` §3: baseline OpenGL 3.3 Core, sin compute;
//! blur por FBO ping-pong con gaussiano separable.

use std::collections::HashMap;

use glow::HasContext;

use crate::error::{CanvasError, Result};
use crate::queue::ShaderKind;

/// Vertex shader común: atributos pos/uv/color + proyección ortográfica.
const VS_QUAD: &str = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
uniform mat4 u_proj;
out vec2 v_uv;
out vec4 v_color;
void main() {
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = u_proj * vec4(a_pos, 0.0, 1.0);
}"#;

/// Relleno plano (color premultiplicado).
const FS_FLAT: &str = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;
out vec4 frag;
void main() { frag = v_color; }"#;

/// Muestreo de textura × tint (premultiplicado).
const FS_TEXTURED: &str = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_tex;
out vec4 frag;
void main() { frag = texture(u_tex, v_uv) * v_color; }"#;

/// Gaussiano separable de 9 taps (5 pesos espejados), dirección por uniform.
const FS_BLUR: &str = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_tex;
uniform vec2 u_dir; // paso en UV por tap
out vec4 frag;
void main() {
    float w[5] = float[5](0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    vec4 sum = texture(u_tex, v_uv) * w[0];
    for (int i = 1; i < 5; i++) {
        sum += texture(u_tex, v_uv + u_dir * float(i)) * w[i];
        sum += texture(u_tex, v_uv - u_dir * float(i)) * w[i];
    }
    frag = sum * v_color;
}"#;

/// Uniforms cacheadas de un programa.
#[derive(Debug, Clone, Copy)]
struct ProgramLocations {
    projection: Option<glow::UniformLocation>,
    texture: Option<glow::UniformLocation>,
    direction: Option<glow::UniformLocation>,
}

/// Compila, enlaza y cachea los programas del pipeline.
pub struct ShaderManager {
    programs: HashMap<ShaderKind, glow::Program>,
    locations: HashMap<ShaderKind, ProgramLocations>,
}

impl ShaderManager {
    /// Compila todos los programas base del pipeline v1.
    pub fn new(gl: &glow::Context) -> Result<Self> {
        let mut this = Self {
            programs: HashMap::new(),
            locations: HashMap::new(),
        };
        this.compile(gl, ShaderKind::Flat, FS_FLAT)?;
        this.compile(gl, ShaderKind::Textured, FS_TEXTURED)?;
        Ok(this)
    }

    /// Compila (o recompila) el programa de un [`ShaderKind`].
    pub fn compile(
        &mut self,
        gl: &glow::Context,
        kind: ShaderKind,
        fragment: &'static str,
    ) -> Result<()> {
        let program = link_program(gl, VS_QUAD, fragment)?;
        let projection = unsafe { gl.get_uniform_location(program, "u_proj") };
        let texture = unsafe { gl.get_uniform_location(program, "u_tex") };
        let direction = unsafe { gl.get_uniform_location(program, "u_dir") };
        self.programs.insert(kind, program);
        self.locations.insert(
            kind,
            ProgramLocations {
                projection,
                texture,
                direction,
            },
        );
        Ok(())
    }

    /// Programa GL asociado a un kind.
    #[must_use]
    pub fn program(&self, kind: ShaderKind) -> glow::Program {
        self.programs[&kind]
    }

    /// Fija la matriz de proyección ortográfica del programa dado.
    ///
    /// `projection` en column-major (16 × f32).
    pub fn set_projection(&self, gl: &glow::Context, kind: ShaderKind, projection: &[f32; 16]) {
        let locations = &self.locations[&kind];
        if let Some(loc) = locations.projection.as_ref() {
            unsafe { gl.uniform_matrix_4_f32_slice(Some(loc), false, projection) };
        }
    }

    /// Fija la unidad de sampler del programa texturizado.
    pub fn set_sampler(&self, gl: &glow::Context, kind: ShaderKind, unit: u32) {
        if let Some(loc) = self.locations[&kind].texture.as_ref() {
            unsafe { gl.uniform_1_u32(Some(loc), unit) };
        }
    }

    /// Fija la dirección de un tap de blur (en UV).
    pub fn set_blur_direction(&self, gl: &glow::Context, dir: (f32, f32)) {
        let Some(loc) = self.locations[&ShaderKind::Blur].direction.as_ref() else {
            return;
        };
        unsafe { gl.uniform_2_f32(Some(loc), dir.0, dir.1) };
    }

    /// Devuelve el programa de blur, compilándolo si es la primera vez.
    pub fn blur_program(&mut self, gl: &glow::Context) -> Result<glow::Program> {
        if !self.programs.contains_key(&ShaderKind::Blur) {
            self.compile(gl, ShaderKind::Blur, FS_BLUR)?;
        }
        Ok(self.programs[&ShaderKind::Blur])
    }
}

fn link_program(
    gl: &glow::Context,
    vs_source: &'static str,
    fs_source: &'static str,
) -> Result<glow::Program> {
    let vs = compile_shader(gl, glow::VERTEX_SHADER, vs_source, "vertex")?;
    let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fs_source, "fragment")?;
    let program = unsafe { gl.create_program() }.map_err(|e| CanvasError::ShaderCompilation {
        stage: "create",
        log: e.to_string(),
    })?;
    unsafe {
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(CanvasError::ShaderCompilation { stage: "link", log });
        }
    }
    Ok(program)
}

fn compile_shader(
    gl: &glow::Context,
    ty: u32,
    source: &'static str,
    stage: &'static str,
) -> Result<glow::Shader> {
    let shader = unsafe { gl.create_shader(ty) }.map_err(|e| CanvasError::ShaderCompilation {
        stage,
        log: e.to_string(),
    })?;
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(CanvasError::ShaderCompilation { stage, log });
        }
    }
    Ok(shader)
}
