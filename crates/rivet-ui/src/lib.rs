//! Rivet UI: widgets por composición, layout, eventos y foco.
//!
//! Almacenamiento en dos niveles: nodo común `WidgetNode` en arena plana +
//! slabs por tipo; hijos por enlaces intrusivos. Jerarquía por `WidgetId`
//! (64 bits: generación | índice). Componer, no heredar.
