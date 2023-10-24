// This file was autogenerated by some hot garbage in the `uniffi` crate.
// Trust me, you don't want to mess with it!

{%- call kt::docstring(ci.namespace_definition(), 0) %}

@file:Suppress("NAME_SHADOWING")

package {{ config.package_name() }};

// Common helper code.
//
// Ideally this would live in a separate .kt file where it can be unittested etc
// in isolation, and perhaps even published as a re-useable package.
//
// However, it's important that the details of how this helper code works (e.g. the
// way that different builtin types are passed across the FFI) exactly match what's
// expected by the Rust code on the other side of the interface. In practice right
// now that means coming from the exact some version of `uniffi` that was used to
// compile the Rust component. The easiest way to ensure this is to bundle the Kotlin
// helpers directly inline like we're doing here.

import com.sun.jna.Library
import com.sun.jna.IntegerType
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.Structure
import com.sun.jna.Callback
import com.sun.jna.ptr.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import java.util.concurrent.ConcurrentHashMap

{%- for req in self.imports() %}
{{ req.render() }}
{%- endfor %}

{% include "RustBufferTemplate.kt" %}
{% include "FfiConverterTemplate.kt" %}
{% include "Helpers.kt" %}

// Contains loading, initialization code,
// and the FFI Function declarations in a com.sun.jna.Library.
{% include "NamespaceLibraryTemplate.kt" %}

// Async support
{%- if ci.has_async_fns() %}
{% include "Async.kt" %}
{%- endif %}

// Public interface members begin here.
{{ type_helper_code }}

{%- for func in ci.function_definitions() %}
{% include "TopLevelFunctionTemplate.kt" %}
{%- endfor %}

{% import "macros.kt" as kt %}
