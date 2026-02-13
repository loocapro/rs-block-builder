/// Tracing macro wrapper for context, level trace
#[macro_export]
macro_rules! context_trace {
    (target: $target:expr, context: $context:expr, $($tts:tt)*) => {
        tracing::trace!(
            target: $target,
            id=%$context.attributes().id,
            ts=$context.attributes().timestamp,
            slot=$context.slot(),
            block=$context.block_number(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for context, level debug
#[macro_export]
macro_rules! context_debug {
    (target: $target:expr, context: $context:expr, $($tts:tt)*) => {
        tracing::debug!(
            target: $target,
            id=%$context.attributes().id,
            ts=$context.attributes().timestamp,
            slot=$context.slot(),
            block=$context.block_number(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for context, level info
#[macro_export]
macro_rules! context_info {
    (target: $target:expr, context: $context:expr, $($tts:tt)*) => {
        tracing::info!(
            target: $target,
            id=%$context.attributes().id,
            ts=$context.attributes().timestamp,
            slot=$context.slot(),
            block=$context.block_number(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for context, level warn
#[macro_export]
macro_rules! context_warn {
    (target: $target:expr, context: $context:expr, $($tts:tt)*) => {
        tracing::warn!(
            target: $target,
            id=%$context.attributes().id.to_string(),
            ts=$context.attributes().timestamp,
            slot=$context.slot(),
            block=$context.block_number(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for context, level error
#[macro_export]
macro_rules! context_error {
    (target: $target:expr, context: $context:expr, $($tts:tt)*) => {
        tracing::error!(
            target: $target,
            id=%$context.attributes().id.to_string(),
            ts=$context.attributes().timestamp,
            slot=$context.slot(),
            block=$context.block_number(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for build objects, level trace
#[macro_export]
macro_rules! build_trace {
    (target: $target:expr, build: $build:expr, $($tts:tt)*) => {
        context_trace!(
            target: $target,
            context: $build.context,
            build_id=%$build.id.to_string(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for build objects, level debug
#[macro_export]
macro_rules! build_debug {
    (target: $target:expr, build: $build:expr, $($tts:tt)*) => {
        context_debug!(
            target: $target,
            context: $build.context,
            build_id=%$build.id.to_string(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for build objects, level info
#[macro_export]
macro_rules! build_info {
    (target: $target:expr, build: $build:expr, $($tts:tt)*) => {
        context_info!(
            target: $target,
            context: $build.context,
            build_id=%$build.id.to_string(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for build objects, level warn
#[macro_export]
macro_rules! build_warn {
    (target: $target:expr, build: $build:expr, $($tts:tt)*) => {
        context_warn!(
            target: $target,
            context: $build.context,
            build_id=%$build.id.to_string(),
            $($tts)*
        )
    }
}

/// Tracing macro wrapper for build objects, level error
#[macro_export]
macro_rules! build_error {
    (target: $target:expr, build: $build:expr, $($tts:tt)*) => {
        context_error!(
            target: $target,
            context: $build.context,
            build_id=%$build.id.to_string(),
            $($tts)*
        )
    }
}
