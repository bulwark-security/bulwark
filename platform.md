<h1><a name="platform">World platform</a></h1>
<ul>
<li>Imports:
<ul>
<li>interface <a href="#wasi:cli_environment_0.2.0"><code>wasi:cli/environment@0.2.0</code></a></li>
<li>interface <a href="#wasi:io_poll_0.2.0"><code>wasi:io/poll@0.2.0</code></a></li>
<li>interface <a href="#wasi:clocks_monotonic_clock_0.2.0"><code>wasi:clocks/monotonic-clock@0.2.0</code></a></li>
<li>interface <a href="#wasi:io_error_0.2.0"><code>wasi:io/error@0.2.0</code></a></li>
<li>interface <a href="#wasi:io_streams_0.2.0"><code>wasi:io/streams@0.2.0</code></a></li>
<li>interface <a href="#wasi:http_types_0.2.0"><code>wasi:http/types@0.2.0</code></a></li>
<li>interface <a href="#wasi:http_outgoing_handler_0.2.0"><code>wasi:http/outgoing-handler@0.2.0</code></a></li>
<li>interface <a href="#bulwark:plugin_types_0.5.0"><code>bulwark:plugin/types@0.5.0</code></a></li>
<li>interface <a href="#bulwark:plugin_config_0.5.0"><code>bulwark:plugin/config@0.5.0</code></a></li>
<li>interface <a href="#bulwark:plugin_redis_0.5.0"><code>bulwark:plugin/redis@0.5.0</code></a></li>
</ul>
</li>
</ul>
<h2><a name="wasi:cli_environment_0.2.0">Import interface wasi:cli/environment@0.2.0</a></h2>
<hr />
<h3>Functions</h3>
<h4><a name="get_environment"><code>get-environment: func</code></a></h4>
<p>Get the POSIX-style environment variables.</p>
<p>Each environment variable is provided as a pair of string variable names
and string value.</p>
<p>Morally, these are a value import, but until value imports are available
in the component model, this import function should return the same
values each time it is called.</p>
<h5>Return values</h5>
<ul>
<li><a name="get_environment.0"></a> list&lt;(<code>string</code>, <code>string</code>)&gt;</li>
</ul>
<h4><a name="get_arguments"><code>get-arguments: func</code></a></h4>
<p>Get the POSIX-style arguments to the program.</p>
<h5>Return values</h5>
<ul>
<li><a name="get_arguments.0"></a> list&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="initial_cwd"><code>initial-cwd: func</code></a></h4>
<p>Return a path that programs should use as their initial current working
directory, interpreting <code>.</code> as shorthand for this.</p>
<h5>Return values</h5>
<ul>
<li><a name="initial_cwd.0"></a> option&lt;<code>string</code>&gt;</li>
</ul>
<h2><a name="wasi:io_poll_0.2.0">Import interface wasi:io/poll@0.2.0</a></h2>
<p>A poll API intended to let users wait for I/O events on multiple handles
at once.</p>
<hr />
<h3>Types</h3>
<h4><a name="pollable"><code>resource pollable</code></a></h4>
<h2><a href="#pollable"><code>pollable</code></a> represents a single I/O event which may be ready, or not.</h2>
<h3>Functions</h3>
<h4><a name="method_pollable.ready"><code>[method]pollable.ready: func</code></a></h4>
<p>Return the readiness of a pollable. This function never blocks.</p>
<p>Returns <code>true</code> when the pollable is ready, and <code>false</code> otherwise.</p>
<h5>Params</h5>
<ul>
<li><a name="method_pollable.ready.self"><code>self</code></a>: borrow&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_pollable.ready.0"></a> <code>bool</code></li>
</ul>
<h4><a name="method_pollable.block"><code>[method]pollable.block: func</code></a></h4>
<p><code>block</code> returns immediately if the pollable is ready, and otherwise
blocks until ready.</p>
<p>This function is equivalent to calling <code>poll.poll</code> on a list
containing only this pollable.</p>
<h5>Params</h5>
<ul>
<li><a name="method_pollable.block.self"><code>self</code></a>: borrow&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="poll"><code>poll: func</code></a></h4>
<p>Poll for completion on a set of pollables.</p>
<p>This function takes a list of pollables, which identify I/O sources of
interest, and waits until one or more of the events is ready for I/O.</p>
<p>The result <code>list&lt;u32&gt;</code> contains one or more indices of handles in the
argument list that is ready for I/O.</p>
<p>If the list contains more elements than can be indexed with a <code>u32</code>
value, this function traps.</p>
<p>A timeout can be implemented by adding a pollable from the
wasi-clocks API to the list.</p>
<p>This function does not return a <code>result</code>; polling in itself does not
do any I/O so it doesn't fail. If any of the I/O sources identified by
the pollables has an error, it is indicated by marking the source as
being reaedy for I/O.</p>
<h5>Params</h5>
<ul>
<li><a name="poll.in"><code>in</code></a>: list&lt;borrow&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="poll.0"></a> list&lt;<code>u32</code>&gt;</li>
</ul>
<h2><a name="wasi:clocks_monotonic_clock_0.2.0">Import interface wasi:clocks/monotonic-clock@0.2.0</a></h2>
<p>WASI Monotonic Clock is a clock API intended to let users measure elapsed
time.</p>
<p>It is intended to be portable at least between Unix-family platforms and
Windows.</p>
<p>A monotonic clock is a clock which has an unspecified initial value, and
successive reads of the clock will produce non-decreasing values.</p>
<p>It is intended for measuring elapsed time.</p>
<hr />
<h3>Types</h3>
<h4><a name="pollable"><code>type pollable</code></a></h4>
<p><a href="#pollable"><a href="#pollable"><code>pollable</code></a></a></p>
<p>
#### <a name="instant">`type instant`</a>
`u64`
<p>An instant in time, in nanoseconds. An instant is relative to an
unspecified initial value, and can only be compared to instances from
the same monotonic-clock.
<h4><a name="duration"><code>type duration</code></a></h4>
<p><code>u64</code></p>
<p>A duration of time, in nanoseconds.
<hr />
<h3>Functions</h3>
<h4><a name="now"><code>now: func</code></a></h4>
<p>Read the current value of the clock.</p>
<p>The clock is monotonic, therefore calling this function repeatedly will
produce a sequence of non-decreasing values.</p>
<h5>Return values</h5>
<ul>
<li><a name="now.0"></a> <a href="#instant"><a href="#instant"><code>instant</code></a></a></li>
</ul>
<h4><a name="resolution"><code>resolution: func</code></a></h4>
<p>Query the resolution of the clock. Returns the duration of time
corresponding to a clock tick.</p>
<h5>Return values</h5>
<ul>
<li><a name="resolution.0"></a> <a href="#duration"><a href="#duration"><code>duration</code></a></a></li>
</ul>
<h4><a name="subscribe_instant"><code>subscribe-instant: func</code></a></h4>
<p>Create a <a href="#pollable"><code>pollable</code></a> which will resolve once the specified instant
occured.</p>
<h5>Params</h5>
<ul>
<li><a name="subscribe_instant.when"><code>when</code></a>: <a href="#instant"><a href="#instant"><code>instant</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="subscribe_instant.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="subscribe_duration"><code>subscribe-duration: func</code></a></h4>
<p>Create a <a href="#pollable"><code>pollable</code></a> which will resolve once the given duration has
elapsed, starting at the time at which this function was called.
occured.</p>
<h5>Params</h5>
<ul>
<li><a name="subscribe_duration.when"><code>when</code></a>: <a href="#duration"><a href="#duration"><code>duration</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="subscribe_duration.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h2><a name="wasi:io_error_0.2.0">Import interface wasi:io/error@0.2.0</a></h2>
<hr />
<h3>Types</h3>
<h4><a name="error"><code>resource error</code></a></h4>
<p>A resource which represents some error information.</p>
<p>The only method provided by this resource is <code>to-debug-string</code>,
which provides some human-readable information about the error.</p>
<p>In the <code>wasi:io</code> package, this resource is returned through the
<code>wasi:io/streams/stream-error</code> type.</p>
<p>To provide more specific error information, other interfaces may
provide functions to further &quot;downcast&quot; this error into more specific
error information. For example, <a href="#error"><code>error</code></a>s returned in streams derived
from filesystem types to be described using the filesystem's own
error-code type, using the function
<code>wasi:filesystem/types/filesystem-error-code</code>, which takes a parameter
<code>borrow&lt;error&gt;</code> and returns
<code>option&lt;wasi:filesystem/types/error-code&gt;</code>.</p>
<h2>The set of functions which can &quot;downcast&quot; an <a href="#error"><code>error</code></a> into a more
concrete type is open.</h2>
<h3>Functions</h3>
<h4><a name="method_error.to_debug_string"><code>[method]error.to-debug-string: func</code></a></h4>
<p>Returns a string that is suitable to assist humans in debugging
this error.</p>
<p>WARNING: The returned string should not be consumed mechanically!
It may change across platforms, hosts, or other implementation
details. Parsing this string is a major platform-compatibility
hazard.</p>
<h5>Params</h5>
<ul>
<li><a name="method_error.to_debug_string.self"><code>self</code></a>: borrow&lt;<a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_error.to_debug_string.0"></a> <code>string</code></li>
</ul>
<h2><a name="wasi:io_streams_0.2.0">Import interface wasi:io/streams@0.2.0</a></h2>
<p>WASI I/O is an I/O abstraction API which is currently focused on providing
stream types.</p>
<p>In the future, the component model is expected to add built-in stream types;
when it does, they are expected to subsume this API.</p>
<hr />
<h3>Types</h3>
<h4><a name="error"><code>type error</code></a></h4>
<p><a href="#error"><a href="#error"><code>error</code></a></a></p>
<p>
#### <a name="pollable">`type pollable`</a>
[`pollable`](#pollable)
<p>
#### <a name="stream_error">`variant stream-error`</a>
<p>An error for input-stream and output-stream operations.</p>
<h5>Variant Cases</h5>
<ul>
<li>
<p><a name="stream_error.last_operation_failed"><code>last-operation-failed</code></a>: own&lt;<a href="#error"><a href="#error"><code>error</code></a></a>&gt;</p>
<p>The last operation (a write or flush) failed before completion.
<p>More information is available in the <a href="#error"><code>error</code></a> payload.</p>
</li>
<li>
<p><a name="stream_error.closed"><code>closed</code></a></p>
<p>The stream is closed: no more input will be accepted by the
stream. A closed output-stream will return this error on all
future operations.
</li>
</ul>
<h4><a name="input_stream"><code>resource input-stream</code></a></h4>
<p>An input bytestream.</p>
<p><a href="#input_stream"><code>input-stream</code></a>s are <em>non-blocking</em> to the extent practical on underlying
platforms. I/O operations always return promptly; if fewer bytes are
promptly available than requested, they return the number of bytes promptly
available, which could even be zero. To wait for data to be available,
use the <code>subscribe</code> function to obtain a <a href="#pollable"><code>pollable</code></a> which can be polled
for using <code>wasi:io/poll</code>.</p>
<h4><a name="output_stream"><code>resource output-stream</code></a></h4>
<p>An output bytestream.</p>
<h2><a href="#output_stream"><code>output-stream</code></a>s are <em>non-blocking</em> to the extent practical on
underlying platforms. Except where specified otherwise, I/O operations also
always return promptly, after the number of bytes that can be written
promptly, which could even be zero. To wait for the stream to be ready to
accept data, the <code>subscribe</code> function to obtain a <a href="#pollable"><code>pollable</code></a> which can be
polled for using <code>wasi:io/poll</code>.</h2>
<h3>Functions</h3>
<h4><a name="method_input_stream.read"><code>[method]input-stream.read: func</code></a></h4>
<p>Perform a non-blocking read from the stream.</p>
<p>When the source of a <code>read</code> is binary data, the bytes from the source
are returned verbatim. When the source of a <code>read</code> is known to the
implementation to be text, bytes containing the UTF-8 encoding of the
text are returned.</p>
<p>This function returns a list of bytes containing the read data,
when successful. The returned list will contain up to <code>len</code> bytes;
it may return fewer than requested, but not more. The list is
empty when no bytes are available for reading at this time. The
pollable given by <code>subscribe</code> will be ready when more bytes are
available.</p>
<p>This function fails with a <a href="#stream_error"><code>stream-error</code></a> when the operation
encounters an error, giving <code>last-operation-failed</code>, or when the
stream is closed, giving <code>closed</code>.</p>
<p>When the caller gives a <code>len</code> of 0, it represents a request to
read 0 bytes. If the stream is still open, this call should
succeed and return an empty list, or otherwise fail with <code>closed</code>.</p>
<p>The <code>len</code> parameter is a <code>u64</code>, which could represent a list of u8 which
is not possible to allocate in wasm32, or not desirable to allocate as
as a return value by the callee. The callee may return a list of bytes
less than <code>len</code> in size while more bytes are available for reading.</p>
<h5>Params</h5>
<ul>
<li><a name="method_input_stream.read.self"><code>self</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_input_stream.read.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_input_stream.read.0"></a> result&lt;list&lt;<code>u8</code>&gt;, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_input_stream.blocking_read"><code>[method]input-stream.blocking-read: func</code></a></h4>
<p>Read bytes from a stream, after blocking until at least one byte can
be read. Except for blocking, behavior is identical to <code>read</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_input_stream.blocking_read.self"><code>self</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_input_stream.blocking_read.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_input_stream.blocking_read.0"></a> result&lt;list&lt;<code>u8</code>&gt;, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_input_stream.skip"><code>[method]input-stream.skip: func</code></a></h4>
<p>Skip bytes from a stream. Returns number of bytes skipped.</p>
<p>Behaves identical to <code>read</code>, except instead of returning a list
of bytes, returns the number of bytes consumed from the stream.</p>
<h5>Params</h5>
<ul>
<li><a name="method_input_stream.skip.self"><code>self</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_input_stream.skip.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_input_stream.skip.0"></a> result&lt;<code>u64</code>, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_input_stream.blocking_skip"><code>[method]input-stream.blocking-skip: func</code></a></h4>
<p>Skip bytes from a stream, after blocking until at least one byte
can be skipped. Except for blocking behavior, identical to <code>skip</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_input_stream.blocking_skip.self"><code>self</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_input_stream.blocking_skip.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_input_stream.blocking_skip.0"></a> result&lt;<code>u64</code>, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_input_stream.subscribe"><code>[method]input-stream.subscribe: func</code></a></h4>
<p>Create a <a href="#pollable"><code>pollable</code></a> which will resolve once either the specified stream
has bytes available to read or the other end of the stream has been
closed.
The created <a href="#pollable"><code>pollable</code></a> is a child resource of the <a href="#input_stream"><code>input-stream</code></a>.
Implementations may trap if the <a href="#input_stream"><code>input-stream</code></a> is dropped before
all derived <a href="#pollable"><code>pollable</code></a>s created with this function are dropped.</p>
<h5>Params</h5>
<ul>
<li><a name="method_input_stream.subscribe.self"><code>self</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_input_stream.subscribe.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.check_write"><code>[method]output-stream.check-write: func</code></a></h4>
<p>Check readiness for writing. This function never blocks.</p>
<p>Returns the number of bytes permitted for the next call to <code>write</code>,
or an error. Calling <code>write</code> with more bytes than this function has
permitted will trap.</p>
<p>When this function returns 0 bytes, the <code>subscribe</code> pollable will
become ready when this function will report at least 1 byte, or an
error.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.check_write.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.check_write.0"></a> result&lt;<code>u64</code>, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.write"><code>[method]output-stream.write: func</code></a></h4>
<p>Perform a write. This function never blocks.</p>
<p>When the destination of a <code>write</code> is binary data, the bytes from
<code>contents</code> are written verbatim. When the destination of a <code>write</code> is
known to the implementation to be text, the bytes of <code>contents</code> are
transcoded from UTF-8 into the encoding of the destination and then
written.</p>
<p>Precondition: check-write gave permit of Ok(n) and contents has a
length of less than or equal to n. Otherwise, this function will trap.</p>
<p>returns Err(closed) without writing if the stream has closed since
the last call to check-write provided a permit.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.write.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.write.contents"><code>contents</code></a>: list&lt;<code>u8</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.write.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.blocking_write_and_flush"><code>[method]output-stream.blocking-write-and-flush: func</code></a></h4>
<p>Perform a write of up to 4096 bytes, and then flush the stream. Block
until all of these operations are complete, or an error occurs.</p>
<p>This is a convenience wrapper around the use of <code>check-write</code>,
<code>subscribe</code>, <code>write</code>, and <code>flush</code>, and is implemented with the
following pseudo-code:</p>
<pre><code class="language-text">let pollable = this.subscribe();
while !contents.is_empty() {
  // Wait for the stream to become writable
  pollable.block();
  let Ok(n) = this.check-write(); // eliding error handling
  let len = min(n, contents.len());
  let (chunk, rest) = contents.split_at(len);
  this.write(chunk  );            // eliding error handling
  contents = rest;
}
this.flush();
// Wait for completion of `flush`
pollable.block();
// Check for any errors that arose during `flush`
let _ = this.check-write();         // eliding error handling
</code></pre>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.blocking_write_and_flush.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.blocking_write_and_flush.contents"><code>contents</code></a>: list&lt;<code>u8</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.blocking_write_and_flush.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.flush"><code>[method]output-stream.flush: func</code></a></h4>
<p>Request to flush buffered output. This function never blocks.</p>
<p>This tells the output-stream that the caller intends any buffered
output to be flushed. the output which is expected to be flushed
is all that has been passed to <code>write</code> prior to this call.</p>
<p>Upon calling this function, the <a href="#output_stream"><code>output-stream</code></a> will not accept any
writes (<code>check-write</code> will return <code>ok(0)</code>) until the flush has
completed. The <code>subscribe</code> pollable will become ready when the
flush has completed and the stream can accept more writes.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.flush.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.flush.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.blocking_flush"><code>[method]output-stream.blocking-flush: func</code></a></h4>
<p>Request to flush buffered output, and block until flush completes
and stream is ready for writing again.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.blocking_flush.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.blocking_flush.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.subscribe"><code>[method]output-stream.subscribe: func</code></a></h4>
<p>Create a <a href="#pollable"><code>pollable</code></a> which will resolve once the output-stream
is ready for more writing, or an error has occured. When this
pollable is ready, <code>check-write</code> will return <code>ok(n)</code> with n&gt;0, or an
error.</p>
<p>If the stream is closed, this pollable is always ready immediately.</p>
<p>The created <a href="#pollable"><code>pollable</code></a> is a child resource of the <a href="#output_stream"><code>output-stream</code></a>.
Implementations may trap if the <a href="#output_stream"><code>output-stream</code></a> is dropped before
all derived <a href="#pollable"><code>pollable</code></a>s created with this function are dropped.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.subscribe.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.subscribe.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.write_zeroes"><code>[method]output-stream.write-zeroes: func</code></a></h4>
<p>Write zeroes to a stream.</p>
<p>This should be used precisely like <code>write</code> with the exact same
preconditions (must use check-write first), but instead of
passing a list of bytes, you simply pass the number of zero-bytes
that should be written.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.write_zeroes.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.write_zeroes.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.write_zeroes.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.blocking_write_zeroes_and_flush"><code>[method]output-stream.blocking-write-zeroes-and-flush: func</code></a></h4>
<p>Perform a write of up to 4096 zeroes, and then flush the stream.
Block until all of these operations are complete, or an error
occurs.</p>
<p>This is a convenience wrapper around the use of <code>check-write</code>,
<code>subscribe</code>, <code>write-zeroes</code>, and <code>flush</code>, and is implemented with
the following pseudo-code:</p>
<pre><code class="language-text">let pollable = this.subscribe();
while num_zeroes != 0 {
  // Wait for the stream to become writable
  pollable.block();
  let Ok(n) = this.check-write(); // eliding error handling
  let len = min(n, num_zeroes);
  this.write-zeroes(len);         // eliding error handling
  num_zeroes -= len;
}
this.flush();
// Wait for completion of `flush`
pollable.block();
// Check for any errors that arose during `flush`
let _ = this.check-write();         // eliding error handling
</code></pre>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.blocking_write_zeroes_and_flush.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.blocking_write_zeroes_and_flush.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.blocking_write_zeroes_and_flush.0"></a> result&lt;_, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.splice"><code>[method]output-stream.splice: func</code></a></h4>
<p>Read from one stream and write to another.</p>
<p>The behavior of splice is equivelant to:</p>
<ol>
<li>calling <code>check-write</code> on the <a href="#output_stream"><code>output-stream</code></a></li>
<li>calling <code>read</code> on the <a href="#input_stream"><code>input-stream</code></a> with the smaller of the
<code>check-write</code> permitted length and the <code>len</code> provided to <code>splice</code></li>
<li>calling <code>write</code> on the <a href="#output_stream"><code>output-stream</code></a> with that read data.</li>
</ol>
<p>Any error reported by the call to <code>check-write</code>, <code>read</code>, or
<code>write</code> ends the splice and reports that error.</p>
<p>This function returns the number of bytes transferred; it may be less
than <code>len</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.splice.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.splice.src"><code>src</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.splice.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.splice.0"></a> result&lt;<code>u64</code>, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_output_stream.blocking_splice"><code>[method]output-stream.blocking-splice: func</code></a></h4>
<p>Read from one stream and write to another, with blocking.</p>
<p>This is similar to <code>splice</code>, except that it blocks until the
<a href="#output_stream"><code>output-stream</code></a> is ready for writing, and the <a href="#input_stream"><code>input-stream</code></a>
is ready for reading, before performing the <code>splice</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_output_stream.blocking_splice.self"><code>self</code></a>: borrow&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.blocking_splice.src"><code>src</code></a>: borrow&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;</li>
<li><a name="method_output_stream.blocking_splice.len"><code>len</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_output_stream.blocking_splice.0"></a> result&lt;<code>u64</code>, <a href="#stream_error"><a href="#stream_error"><code>stream-error</code></a></a>&gt;</li>
</ul>
<h2><a name="wasi:http_types_0.2.0">Import interface wasi:http/types@0.2.0</a></h2>
<p>This interface defines all of the types and methods for implementing
HTTP Requests and Responses, both incoming and outgoing, as well as
their headers, trailers, and bodies.</p>
<hr />
<h3>Types</h3>
<h4><a name="duration"><code>type duration</code></a></h4>
<p><a href="#duration"><a href="#duration"><code>duration</code></a></a></p>
<p>
#### <a name="input_stream">`type input-stream`</a>
[`input-stream`](#input_stream)
<p>
#### <a name="output_stream">`type output-stream`</a>
[`output-stream`](#output_stream)
<p>
#### <a name="io_error">`type io-error`</a>
[`error`](#error)
<p>
#### <a name="pollable">`type pollable`</a>
[`pollable`](#pollable)
<p>
#### <a name="method">`variant method`</a>
<p>This type corresponds to HTTP standard Methods.</p>
<h5>Variant Cases</h5>
<ul>
<li><a name="method.get"><a href="#get"><code>get</code></a></a></li>
<li><a name="method.head"><code>head</code></a></li>
<li><a name="method.post"><code>post</code></a></li>
<li><a name="method.put"><code>put</code></a></li>
<li><a name="method.delete"><code>delete</code></a></li>
<li><a name="method.connect"><code>connect</code></a></li>
<li><a name="method.options"><code>options</code></a></li>
<li><a name="method.trace"><code>trace</code></a></li>
<li><a name="method.patch"><code>patch</code></a></li>
<li><a name="method.other"><code>other</code></a>: <code>string</code></li>
</ul>
<h4><a name="scheme"><code>variant scheme</code></a></h4>
<p>This type corresponds to HTTP standard Related Schemes.</p>
<h5>Variant Cases</h5>
<ul>
<li><a name="scheme.http"><code>HTTP</code></a></li>
<li><a name="scheme.https"><code>HTTPS</code></a></li>
<li><a name="scheme.other"><code>other</code></a>: <code>string</code></li>
</ul>
<h4><a name="dns_error_payload"><code>record DNS-error-payload</code></a></h4>
<p>Defines the case payload type for <code>DNS-error</code> above:</p>
<h5>Record Fields</h5>
<ul>
<li><a name="dns_error_payload.rcode"><code>rcode</code></a>: option&lt;<code>string</code>&gt;</li>
<li><a name="dns_error_payload.info_code"><code>info-code</code></a>: option&lt;<code>u16</code>&gt;</li>
</ul>
<h4><a name="tls_alert_received_payload"><code>record TLS-alert-received-payload</code></a></h4>
<p>Defines the case payload type for <code>TLS-alert-received</code> above:</p>
<h5>Record Fields</h5>
<ul>
<li><a name="tls_alert_received_payload.alert_id"><code>alert-id</code></a>: option&lt;<code>u8</code>&gt;</li>
<li><a name="tls_alert_received_payload.alert_message"><code>alert-message</code></a>: option&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="field_size_payload"><code>record field-size-payload</code></a></h4>
<p>Defines the case payload type for <code>HTTP-response-{header,trailer}-size</code> above:</p>
<h5>Record Fields</h5>
<ul>
<li><a name="field_size_payload.field_name"><code>field-name</code></a>: option&lt;<code>string</code>&gt;</li>
<li><a name="field_size_payload.field_size"><code>field-size</code></a>: option&lt;<code>u32</code>&gt;</li>
</ul>
<h4><a name="error_code"><code>variant error-code</code></a></h4>
<p>These cases are inspired by the IANA HTTP Proxy Error Types:
https://www.iana.org/assignments/http-proxy-status/http-proxy-status.xhtml#table-http-proxy-error-types</p>
<h5>Variant Cases</h5>
<ul>
<li><a name="error_code.dns_timeout"><code>DNS-timeout</code></a></li>
<li><a name="error_code.dns_error"><code>DNS-error</code></a>: <a href="#dns_error_payload"><a href="#dns_error_payload"><code>DNS-error-payload</code></a></a></li>
<li><a name="error_code.destination_not_found"><code>destination-not-found</code></a></li>
<li><a name="error_code.destination_unavailable"><code>destination-unavailable</code></a></li>
<li><a name="error_code.destination_ip_prohibited"><code>destination-IP-prohibited</code></a></li>
<li><a name="error_code.destination_ip_unroutable"><code>destination-IP-unroutable</code></a></li>
<li><a name="error_code.connection_refused"><code>connection-refused</code></a></li>
<li><a name="error_code.connection_terminated"><code>connection-terminated</code></a></li>
<li><a name="error_code.connection_timeout"><code>connection-timeout</code></a></li>
<li><a name="error_code.connection_read_timeout"><code>connection-read-timeout</code></a></li>
<li><a name="error_code.connection_write_timeout"><code>connection-write-timeout</code></a></li>
<li><a name="error_code.connection_limit_reached"><code>connection-limit-reached</code></a></li>
<li><a name="error_code.tls_protocol_error"><code>TLS-protocol-error</code></a></li>
<li><a name="error_code.tls_certificate_error"><code>TLS-certificate-error</code></a></li>
<li><a name="error_code.tls_alert_received"><code>TLS-alert-received</code></a>: <a href="#tls_alert_received_payload"><a href="#tls_alert_received_payload"><code>TLS-alert-received-payload</code></a></a></li>
<li><a name="error_code.http_request_denied"><code>HTTP-request-denied</code></a></li>
<li><a name="error_code.http_request_length_required"><code>HTTP-request-length-required</code></a></li>
<li><a name="error_code.http_request_body_size"><code>HTTP-request-body-size</code></a>: option&lt;<code>u64</code>&gt;</li>
<li><a name="error_code.http_request_method_invalid"><code>HTTP-request-method-invalid</code></a></li>
<li><a name="error_code.http_request_uri_invalid"><code>HTTP-request-URI-invalid</code></a></li>
<li><a name="error_code.http_request_uri_too_long"><code>HTTP-request-URI-too-long</code></a></li>
<li><a name="error_code.http_request_header_section_size"><code>HTTP-request-header-section-size</code></a>: option&lt;<code>u32</code>&gt;</li>
<li><a name="error_code.http_request_header_size"><code>HTTP-request-header-size</code></a>: option&lt;<a href="#field_size_payload"><a href="#field_size_payload"><code>field-size-payload</code></a></a>&gt;</li>
<li><a name="error_code.http_request_trailer_section_size"><code>HTTP-request-trailer-section-size</code></a>: option&lt;<code>u32</code>&gt;</li>
<li><a name="error_code.http_request_trailer_size"><code>HTTP-request-trailer-size</code></a>: <a href="#field_size_payload"><a href="#field_size_payload"><code>field-size-payload</code></a></a></li>
<li><a name="error_code.http_response_incomplete"><code>HTTP-response-incomplete</code></a></li>
<li><a name="error_code.http_response_header_section_size"><code>HTTP-response-header-section-size</code></a>: option&lt;<code>u32</code>&gt;</li>
<li><a name="error_code.http_response_header_size"><code>HTTP-response-header-size</code></a>: <a href="#field_size_payload"><a href="#field_size_payload"><code>field-size-payload</code></a></a></li>
<li><a name="error_code.http_response_body_size"><code>HTTP-response-body-size</code></a>: option&lt;<code>u64</code>&gt;</li>
<li><a name="error_code.http_response_trailer_section_size"><code>HTTP-response-trailer-section-size</code></a>: option&lt;<code>u32</code>&gt;</li>
<li><a name="error_code.http_response_trailer_size"><code>HTTP-response-trailer-size</code></a>: <a href="#field_size_payload"><a href="#field_size_payload"><code>field-size-payload</code></a></a></li>
<li><a name="error_code.http_response_transfer_coding"><code>HTTP-response-transfer-coding</code></a>: option&lt;<code>string</code>&gt;</li>
<li><a name="error_code.http_response_content_coding"><code>HTTP-response-content-coding</code></a>: option&lt;<code>string</code>&gt;</li>
<li><a name="error_code.http_response_timeout"><code>HTTP-response-timeout</code></a></li>
<li><a name="error_code.http_upgrade_failed"><code>HTTP-upgrade-failed</code></a></li>
<li><a name="error_code.http_protocol_error"><code>HTTP-protocol-error</code></a></li>
<li><a name="error_code.loop_detected"><code>loop-detected</code></a></li>
<li><a name="error_code.configuration_error"><code>configuration-error</code></a></li>
<li><a name="error_code.internal_error"><code>internal-error</code></a>: option&lt;<code>string</code>&gt;<p>This is a catch-all error for anything that doesn't fit cleanly into a
more specific case. It also includes an optional string for an
unstructured description of the error. Users should not depend on the
string for diagnosing errors, as it's not required to be consistent
between implementations.
</li>
</ul>
<h4><a name="header_error"><code>variant header-error</code></a></h4>
<p>This type enumerates the different kinds of errors that may occur when
setting or appending to a <a href="#fields"><code>fields</code></a> resource.</p>
<h5>Variant Cases</h5>
<ul>
<li>
<p><a name="header_error.invalid_syntax"><code>invalid-syntax</code></a></p>
<p>This error indicates that a `field-key` or `field-value` was
syntactically invalid when used with an operation that sets headers in a
`fields`.
</li>
<li>
<p><a name="header_error.forbidden"><code>forbidden</code></a></p>
<p>This error indicates that a forbidden `field-key` was used when trying
to set a header in a `fields`.
</li>
<li>
<p><a name="header_error.immutable"><code>immutable</code></a></p>
<p>This error indicates that the operation on the `fields` was not
permitted because the fields are immutable.
</li>
</ul>
<h4><a name="field_key"><code>type field-key</code></a></h4>
<p><code>string</code></p>
<p>Field keys are always strings.
<h4><a name="field_value"><code>type field-value</code></a></h4>
<p><a href="#field_value"><a href="#field_value"><code>field-value</code></a></a></p>
<p>Field values should always be ASCII strings. However, in
reality, HTTP implementations often have to interpret malformed values,
so they are provided as a list of bytes.
<h4><a name="fields"><code>resource fields</code></a></h4>
<p>This following block defines the <a href="#fields"><code>fields</code></a> resource which corresponds to
HTTP standard Fields. Fields are a common representation used for both
Headers and Trailers.</p>
<p>A <a href="#fields"><code>fields</code></a> may be mutable or immutable. A <a href="#fields"><code>fields</code></a> created using the
constructor, <code>from-list</code>, or <code>clone</code> will be mutable, but a <a href="#fields"><code>fields</code></a>
resource given by other means (including, but not limited to,
<code>incoming-request.headers</code>, <code>outgoing-request.headers</code>) might be be
immutable. In an immutable fields, the <a href="#set"><code>set</code></a>, <code>append</code>, and <code>delete</code>
operations will fail with <code>header-error.immutable</code>.</p>
<h4><a name="headers"><code>type headers</code></a></h4>
<p><a href="#fields"><a href="#fields"><code>fields</code></a></a></p>
<p>Headers is an alias for Fields.
<h4><a name="trailers"><code>type trailers</code></a></h4>
<p><a href="#fields"><a href="#fields"><code>fields</code></a></a></p>
<p>Trailers is an alias for Fields.
<h4><a name="incoming_request"><code>resource incoming-request</code></a></h4>
<p>Represents an incoming HTTP Request.</p>
<h4><a name="outgoing_request"><code>resource outgoing-request</code></a></h4>
<p>Represents an outgoing HTTP Request.</p>
<h4><a name="request_options"><code>resource request-options</code></a></h4>
<p>Parameters for making an HTTP Request. Each of these parameters is
currently an optional timeout applicable to the transport layer of the
HTTP protocol.</p>
<p>These timeouts are separate from any the user may use to bound a
blocking call to <code>wasi:io/poll.poll</code>.</p>
<h4><a name="response_outparam"><code>resource response-outparam</code></a></h4>
<p>Represents the ability to send an HTTP Response.</p>
<p>This resource is used by the <code>wasi:http/incoming-handler</code> interface to
allow a Response to be sent corresponding to the Request provided as the
other argument to <code>incoming-handler.handle</code>.</p>
<h4><a name="status_code"><code>type status-code</code></a></h4>
<p><code>u16</code></p>
<p>This type corresponds to the HTTP standard Status Code.
<h4><a name="incoming_response"><code>resource incoming-response</code></a></h4>
<p>Represents an incoming HTTP Response.</p>
<h4><a name="incoming_body"><code>resource incoming-body</code></a></h4>
<p>Represents an incoming HTTP Request or Response's Body.</p>
<p>A body has both its contents - a stream of bytes - and a (possibly
empty) set of trailers, indicating that the full contents of the
body have been received. This resource represents the contents as
an <a href="#input_stream"><code>input-stream</code></a> and the delivery of trailers as a <a href="#future_trailers"><code>future-trailers</code></a>,
and ensures that the user of this interface may only be consuming either
the body contents or waiting on trailers at any given time.</p>
<h4><a name="future_trailers"><code>resource future-trailers</code></a></h4>
<p>Represents a future which may eventaully return trailers, or an error.</p>
<p>In the case that the incoming HTTP Request or Response did not have any
trailers, this future will resolve to the empty set of trailers once the
complete Request or Response body has been received.</p>
<h4><a name="outgoing_response"><code>resource outgoing-response</code></a></h4>
<p>Represents an outgoing HTTP Response.</p>
<h4><a name="outgoing_body"><code>resource outgoing-body</code></a></h4>
<p>Represents an outgoing HTTP Request or Response's Body.</p>
<p>A body has both its contents - a stream of bytes - and a (possibly
empty) set of trailers, inducating the full contents of the body
have been sent. This resource represents the contents as an
<a href="#output_stream"><code>output-stream</code></a> child resource, and the completion of the body (with
optional trailers) with a static function that consumes the
<a href="#outgoing_body"><code>outgoing-body</code></a> resource, and ensures that the user of this interface
may not write to the body contents after the body has been finished.</p>
<p>If the user code drops this resource, as opposed to calling the static
method <code>finish</code>, the implementation should treat the body as incomplete,
and that an error has occured. The implementation should propogate this
error to the HTTP protocol by whatever means it has available,
including: corrupting the body on the wire, aborting the associated
Request, or sending a late status code for the Response.</p>
<h4><a name="future_incoming_response"><code>resource future-incoming-response</code></a></h4>
<p>Represents a future which may eventaully return an incoming HTTP
Response, or an error.</p>
<h2>This resource is returned by the <code>wasi:http/outgoing-handler</code> interface to
provide the HTTP Response corresponding to the sent Request.</h2>
<h3>Functions</h3>
<h4><a name="http_error_code"><code>http-error-code: func</code></a></h4>
<p>Attempts to extract a http-related <a href="#error"><code>error</code></a> from the wasi:io <a href="#error"><code>error</code></a>
provided.</p>
<p>Stream operations which return
<code>wasi:io/stream/stream-error::last-operation-failed</code> have a payload of
type <code>wasi:io/error/error</code> with more information about the operation
that failed. This payload can be passed through to this function to see
if there's http-related information about the error to return.</p>
<p>Note that this function is fallible because not all io-errors are
http-related errors.</p>
<h5>Params</h5>
<ul>
<li><a name="http_error_code.err"><code>err</code></a>: borrow&lt;<a href="#io_error"><a href="#io_error"><code>io-error</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="http_error_code.0"></a> option&lt;<a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;</li>
</ul>
<h4><a name="constructor_fields"><code>[constructor]fields: func</code></a></h4>
<p>Construct an empty HTTP Fields.</p>
<p>The resulting <a href="#fields"><code>fields</code></a> is mutable.</p>
<h5>Return values</h5>
<ul>
<li><a name="constructor_fields.0"></a> own&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
</ul>
<h4><a name="static_fields.from_list"><code>[static]fields.from-list: func</code></a></h4>
<p>Construct an HTTP Fields.</p>
<p>The resulting <a href="#fields"><code>fields</code></a> is mutable.</p>
<p>The list represents each key-value pair in the Fields. Keys
which have multiple values are represented by multiple entries in this
list with the same key.</p>
<p>The tuple is a pair of the field key, represented as a string, and
Value, represented as a list of bytes. In a valid Fields, all keys
and values are valid UTF-8 strings. However, values are not always
well-formed, so they are represented as a raw list of bytes.</p>
<p>An error result will be returned if any header or value was
syntactically invalid, or if a header was forbidden.</p>
<h5>Params</h5>
<ul>
<li><a name="static_fields.from_list.entries"><code>entries</code></a>: list&lt;(<a href="#field_key"><a href="#field_key"><code>field-key</code></a></a>, <a href="#field_value"><a href="#field_value"><code>field-value</code></a></a>)&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="static_fields.from_list.0"></a> result&lt;own&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;, <a href="#header_error"><a href="#header_error"><code>header-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_fields.get"><code>[method]fields.get: func</code></a></h4>
<p>Get all of the values corresponding to a key. If the key is not present
in this <a href="#fields"><code>fields</code></a>, an empty list is returned. However, if the key is
present but empty, this is represented by a list with one or more
empty field-values present.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.get.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
<li><a name="method_fields.get.name"><code>name</code></a>: <a href="#field_key"><a href="#field_key"><code>field-key</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.get.0"></a> list&lt;<a href="#field_value"><a href="#field_value"><code>field-value</code></a></a>&gt;</li>
</ul>
<h4><a name="method_fields.has"><code>[method]fields.has: func</code></a></h4>
<p>Returns <code>true</code> when the key is present in this <a href="#fields"><code>fields</code></a>. If the key is
syntactically invalid, <code>false</code> is returned.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.has.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
<li><a name="method_fields.has.name"><code>name</code></a>: <a href="#field_key"><a href="#field_key"><code>field-key</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.has.0"></a> <code>bool</code></li>
</ul>
<h4><a name="method_fields.set"><code>[method]fields.set: func</code></a></h4>
<p>Set all of the values for a key. Clears any existing values for that
key, if they have been set.</p>
<p>Fails with <code>header-error.immutable</code> if the <a href="#fields"><code>fields</code></a> are immutable.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.set.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
<li><a name="method_fields.set.name"><code>name</code></a>: <a href="#field_key"><a href="#field_key"><code>field-key</code></a></a></li>
<li><a name="method_fields.set.value"><a href="#value"><code>value</code></a></a>: list&lt;<a href="#field_value"><a href="#field_value"><code>field-value</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.set.0"></a> result&lt;_, <a href="#header_error"><a href="#header_error"><code>header-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_fields.delete"><code>[method]fields.delete: func</code></a></h4>
<p>Delete all values for a key. Does nothing if no values for the key
exist.</p>
<p>Fails with <code>header-error.immutable</code> if the <a href="#fields"><code>fields</code></a> are immutable.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.delete.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
<li><a name="method_fields.delete.name"><code>name</code></a>: <a href="#field_key"><a href="#field_key"><code>field-key</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.delete.0"></a> result&lt;_, <a href="#header_error"><a href="#header_error"><code>header-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_fields.append"><code>[method]fields.append: func</code></a></h4>
<p>Append a value for a key. Does not change or delete any existing
values for that key.</p>
<p>Fails with <code>header-error.immutable</code> if the <a href="#fields"><code>fields</code></a> are immutable.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.append.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
<li><a name="method_fields.append.name"><code>name</code></a>: <a href="#field_key"><a href="#field_key"><code>field-key</code></a></a></li>
<li><a name="method_fields.append.value"><a href="#value"><code>value</code></a></a>: <a href="#field_value"><a href="#field_value"><code>field-value</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.append.0"></a> result&lt;_, <a href="#header_error"><a href="#header_error"><code>header-error</code></a></a>&gt;</li>
</ul>
<h4><a name="method_fields.entries"><code>[method]fields.entries: func</code></a></h4>
<p>Retrieve the full set of keys and values in the Fields. Like the
constructor, the list represents each key-value pair.</p>
<p>The outer list represents each key-value pair in the Fields. Keys
which have multiple values are represented by multiple entries in this
list with the same key.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.entries.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.entries.0"></a> list&lt;(<a href="#field_key"><a href="#field_key"><code>field-key</code></a></a>, <a href="#field_value"><a href="#field_value"><code>field-value</code></a></a>)&gt;</li>
</ul>
<h4><a name="method_fields.clone"><code>[method]fields.clone: func</code></a></h4>
<p>Make a deep copy of the Fields. Equivelant in behavior to calling the
<a href="#fields"><code>fields</code></a> constructor on the return value of <code>entries</code>. The resulting
<a href="#fields"><code>fields</code></a> is mutable.</p>
<h5>Params</h5>
<ul>
<li><a name="method_fields.clone.self"><code>self</code></a>: borrow&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_fields.clone.0"></a> own&lt;<a href="#fields"><a href="#fields"><code>fields</code></a></a>&gt;</li>
</ul>
<h4><a name="method_incoming_request.method"><code>[method]incoming-request.method: func</code></a></h4>
<p>Returns the method of the incoming request.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.method.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.method.0"></a> <a href="#method"><a href="#method"><code>method</code></a></a></li>
</ul>
<h4><a name="method_incoming_request.path_with_query"><code>[method]incoming-request.path-with-query: func</code></a></h4>
<p>Returns the path with query parameters from the request, as a string.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.path_with_query.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.path_with_query.0"></a> option&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="method_incoming_request.scheme"><code>[method]incoming-request.scheme: func</code></a></h4>
<p>Returns the protocol scheme from the request.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.scheme.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.scheme.0"></a> option&lt;<a href="#scheme"><a href="#scheme"><code>scheme</code></a></a>&gt;</li>
</ul>
<h4><a name="method_incoming_request.authority"><code>[method]incoming-request.authority: func</code></a></h4>
<p>Returns the authority from the request, if it was present.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.authority.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.authority.0"></a> option&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="method_incoming_request.headers"><code>[method]incoming-request.headers: func</code></a></h4>
<p>Get the <a href="#headers"><code>headers</code></a> associated with the request.</p>
<p>The returned <a href="#headers"><code>headers</code></a> resource is immutable: <a href="#set"><code>set</code></a>, <code>append</code>, and
<code>delete</code> operations will fail with <code>header-error.immutable</code>.</p>
<p>The <a href="#headers"><code>headers</code></a> returned are a child resource: it must be dropped before
the parent <a href="#incoming_request"><code>incoming-request</code></a> is dropped. Dropping this
<a href="#incoming_request"><code>incoming-request</code></a> before all children are dropped will trap.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.headers.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.headers.0"></a> own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h4><a name="method_incoming_request.consume"><code>[method]incoming-request.consume: func</code></a></h4>
<p>Gives the <a href="#incoming_body"><code>incoming-body</code></a> associated with this request. Will only
return success at most once, and subsequent calls will return error.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_request.consume.self"><code>self</code></a>: borrow&lt;<a href="#incoming_request"><a href="#incoming_request"><code>incoming-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_request.consume.0"></a> result&lt;own&lt;<a href="#incoming_body"><a href="#incoming_body"><code>incoming-body</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="constructor_outgoing_request"><code>[constructor]outgoing-request: func</code></a></h4>
<p>Construct a new <a href="#outgoing_request"><code>outgoing-request</code></a> with a default <a href="#method"><code>method</code></a> of <code>GET</code>, and
<code>none</code> values for <code>path-with-query</code>, <a href="#scheme"><code>scheme</code></a>, and <code>authority</code>.</p>
<ul>
<li><a href="#headers"><code>headers</code></a> is the HTTP Headers for the Request.</li>
</ul>
<p>It is possible to construct, or manipulate with the accessor functions
below, an <a href="#outgoing_request"><code>outgoing-request</code></a> with an invalid combination of <a href="#scheme"><code>scheme</code></a>
and <code>authority</code>, or <a href="#headers"><code>headers</code></a> which are not permitted to be sent.
It is the obligation of the <code>outgoing-handler.handle</code> implementation
to reject invalid constructions of <a href="#outgoing_request"><code>outgoing-request</code></a>.</p>
<h5>Params</h5>
<ul>
<li><a name="constructor_outgoing_request.headers"><a href="#headers"><code>headers</code></a></a>: own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="constructor_outgoing_request.0"></a> own&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h4><a name="method_outgoing_request.body"><code>[method]outgoing-request.body: func</code></a></h4>
<p>Returns the resource corresponding to the outgoing Body for this
Request.</p>
<p>Returns success on the first call: the <a href="#outgoing_body"><code>outgoing-body</code></a> resource for
this <a href="#outgoing_request"><code>outgoing-request</code></a> can be retrieved at most once. Subsequent
calls will return error.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.body.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.body.0"></a> result&lt;own&lt;<a href="#outgoing_body"><a href="#outgoing_body"><code>outgoing-body</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="method_outgoing_request.method"><code>[method]outgoing-request.method: func</code></a></h4>
<p>Get the Method for the Request.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.method.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.method.0"></a> <a href="#method"><a href="#method"><code>method</code></a></a></li>
</ul>
<h4><a name="method_outgoing_request.set_method"><code>[method]outgoing-request.set-method: func</code></a></h4>
<p>Set the Method for the Request. Fails if the string present in a
<code>method.other</code> argument is not a syntactically valid method.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.set_method.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
<li><a name="method_outgoing_request.set_method.method"><a href="#method"><code>method</code></a></a>: <a href="#method"><a href="#method"><code>method</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.set_method.0"></a> result</li>
</ul>
<h4><a name="method_outgoing_request.path_with_query"><code>[method]outgoing-request.path-with-query: func</code></a></h4>
<p>Get the combination of the HTTP Path and Query for the Request.
When <code>none</code>, this represents an empty Path and empty Query.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.path_with_query.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.path_with_query.0"></a> option&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="method_outgoing_request.set_path_with_query"><code>[method]outgoing-request.set-path-with-query: func</code></a></h4>
<p>Set the combination of the HTTP Path and Query for the Request.
When <code>none</code>, this represents an empty Path and empty Query. Fails is the
string given is not a syntactically valid path and query uri component.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.set_path_with_query.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
<li><a name="method_outgoing_request.set_path_with_query.path_with_query"><code>path-with-query</code></a>: option&lt;<code>string</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.set_path_with_query.0"></a> result</li>
</ul>
<h4><a name="method_outgoing_request.scheme"><code>[method]outgoing-request.scheme: func</code></a></h4>
<p>Get the HTTP Related Scheme for the Request. When <code>none</code>, the
implementation may choose an appropriate default scheme.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.scheme.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.scheme.0"></a> option&lt;<a href="#scheme"><a href="#scheme"><code>scheme</code></a></a>&gt;</li>
</ul>
<h4><a name="method_outgoing_request.set_scheme"><code>[method]outgoing-request.set-scheme: func</code></a></h4>
<p>Set the HTTP Related Scheme for the Request. When <code>none</code>, the
implementation may choose an appropriate default scheme. Fails if the
string given is not a syntactically valid uri scheme.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.set_scheme.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
<li><a name="method_outgoing_request.set_scheme.scheme"><a href="#scheme"><code>scheme</code></a></a>: option&lt;<a href="#scheme"><a href="#scheme"><code>scheme</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.set_scheme.0"></a> result</li>
</ul>
<h4><a name="method_outgoing_request.authority"><code>[method]outgoing-request.authority: func</code></a></h4>
<p>Get the HTTP Authority for the Request. A value of <code>none</code> may be used
with Related Schemes which do not require an Authority. The HTTP and
HTTPS schemes always require an authority.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.authority.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.authority.0"></a> option&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="method_outgoing_request.set_authority"><code>[method]outgoing-request.set-authority: func</code></a></h4>
<p>Set the HTTP Authority for the Request. A value of <code>none</code> may be used
with Related Schemes which do not require an Authority. The HTTP and
HTTPS schemes always require an authority. Fails if the string given is
not a syntactically valid uri authority.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.set_authority.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
<li><a name="method_outgoing_request.set_authority.authority"><code>authority</code></a>: option&lt;<code>string</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.set_authority.0"></a> result</li>
</ul>
<h4><a name="method_outgoing_request.headers"><code>[method]outgoing-request.headers: func</code></a></h4>
<p>Get the headers associated with the Request.</p>
<p>The returned <a href="#headers"><code>headers</code></a> resource is immutable: <a href="#set"><code>set</code></a>, <code>append</code>, and
<code>delete</code> operations will fail with <code>header-error.immutable</code>.</p>
<p>This headers resource is a child: it must be dropped before the parent
<a href="#outgoing_request"><code>outgoing-request</code></a> is dropped, or its ownership is transfered to
another component by e.g. <code>outgoing-handler.handle</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_request.headers.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_request.headers.0"></a> own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h4><a name="constructor_request_options"><code>[constructor]request-options: func</code></a></h4>
<p>Construct a default <a href="#request_options"><code>request-options</code></a> value.</p>
<h5>Return values</h5>
<ul>
<li><a name="constructor_request_options.0"></a> own&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
</ul>
<h4><a name="method_request_options.connect_timeout"><code>[method]request-options.connect-timeout: func</code></a></h4>
<p>The timeout for the initial connect to the HTTP Server.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.connect_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.connect_timeout.0"></a> option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h4><a name="method_request_options.set_connect_timeout"><code>[method]request-options.set-connect-timeout: func</code></a></h4>
<p>Set the timeout for the initial connect to the HTTP Server. An error
return value indicates that this timeout is not supported.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.set_connect_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
<li><a name="method_request_options.set_connect_timeout.duration"><a href="#duration"><code>duration</code></a></a>: option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.set_connect_timeout.0"></a> result</li>
</ul>
<h4><a name="method_request_options.first_byte_timeout"><code>[method]request-options.first-byte-timeout: func</code></a></h4>
<p>The timeout for receiving the first byte of the Response body.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.first_byte_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.first_byte_timeout.0"></a> option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h4><a name="method_request_options.set_first_byte_timeout"><code>[method]request-options.set-first-byte-timeout: func</code></a></h4>
<p>Set the timeout for receiving the first byte of the Response body. An
error return value indicates that this timeout is not supported.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.set_first_byte_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
<li><a name="method_request_options.set_first_byte_timeout.duration"><a href="#duration"><code>duration</code></a></a>: option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.set_first_byte_timeout.0"></a> result</li>
</ul>
<h4><a name="method_request_options.between_bytes_timeout"><code>[method]request-options.between-bytes-timeout: func</code></a></h4>
<p>The timeout for receiving subsequent chunks of bytes in the Response
body stream.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.between_bytes_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.between_bytes_timeout.0"></a> option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h4><a name="method_request_options.set_between_bytes_timeout"><code>[method]request-options.set-between-bytes-timeout: func</code></a></h4>
<p>Set the timeout for receiving subsequent chunks of bytes in the Response
body stream. An error return value indicates that this timeout is not
supported.</p>
<h5>Params</h5>
<ul>
<li><a name="method_request_options.set_between_bytes_timeout.self"><code>self</code></a>: borrow&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;</li>
<li><a name="method_request_options.set_between_bytes_timeout.duration"><a href="#duration"><code>duration</code></a></a>: option&lt;<a href="#duration"><a href="#duration"><code>duration</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_request_options.set_between_bytes_timeout.0"></a> result</li>
</ul>
<h4><a name="static_response_outparam.set"><code>[static]response-outparam.set: func</code></a></h4>
<p>Set the value of the <a href="#response_outparam"><code>response-outparam</code></a> to either send a response,
or indicate an error.</p>
<p>This method consumes the <a href="#response_outparam"><code>response-outparam</code></a> to ensure that it is
called at most once. If it is never called, the implementation
will respond with an error.</p>
<p>The user may provide an <a href="#error"><code>error</code></a> to <code>response</code> to allow the
implementation determine how to respond with an HTTP error response.</p>
<h5>Params</h5>
<ul>
<li><a name="static_response_outparam.set.param"><a href="#param"><code>param</code></a></a>: own&lt;<a href="#response_outparam"><a href="#response_outparam"><code>response-outparam</code></a></a>&gt;</li>
<li><a name="static_response_outparam.set.response"><code>response</code></a>: result&lt;own&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;, <a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;</li>
</ul>
<h4><a name="method_incoming_response.status"><code>[method]incoming-response.status: func</code></a></h4>
<p>Returns the status code from the incoming response.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_response.status.self"><code>self</code></a>: borrow&lt;<a href="#incoming_response"><a href="#incoming_response"><code>incoming-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_response.status.0"></a> <a href="#status_code"><a href="#status_code"><code>status-code</code></a></a></li>
</ul>
<h4><a name="method_incoming_response.headers"><code>[method]incoming-response.headers: func</code></a></h4>
<p>Returns the headers from the incoming response.</p>
<p>The returned <a href="#headers"><code>headers</code></a> resource is immutable: <a href="#set"><code>set</code></a>, <code>append</code>, and
<code>delete</code> operations will fail with <code>header-error.immutable</code>.</p>
<p>This headers resource is a child: it must be dropped before the parent
<a href="#incoming_response"><code>incoming-response</code></a> is dropped.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_response.headers.self"><code>self</code></a>: borrow&lt;<a href="#incoming_response"><a href="#incoming_response"><code>incoming-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_response.headers.0"></a> own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h4><a name="method_incoming_response.consume"><code>[method]incoming-response.consume: func</code></a></h4>
<p>Returns the incoming body. May be called at most once. Returns error
if called additional times.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_response.consume.self"><code>self</code></a>: borrow&lt;<a href="#incoming_response"><a href="#incoming_response"><code>incoming-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_response.consume.0"></a> result&lt;own&lt;<a href="#incoming_body"><a href="#incoming_body"><code>incoming-body</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="method_incoming_body.stream"><code>[method]incoming-body.stream: func</code></a></h4>
<p>Returns the contents of the body, as a stream of bytes.</p>
<p>Returns success on first call: the stream representing the contents
can be retrieved at most once. Subsequent calls will return error.</p>
<p>The returned <a href="#input_stream"><code>input-stream</code></a> resource is a child: it must be dropped
before the parent <a href="#incoming_body"><code>incoming-body</code></a> is dropped, or consumed by
<code>incoming-body.finish</code>.</p>
<p>This invariant ensures that the implementation can determine whether
the user is consuming the contents of the body, waiting on the
<a href="#future_trailers"><code>future-trailers</code></a> to be ready, or neither. This allows for network
backpressure is to be applied when the user is consuming the body,
and for that backpressure to not inhibit delivery of the trailers if
the user does not read the entire body.</p>
<h5>Params</h5>
<ul>
<li><a name="method_incoming_body.stream.self"><code>self</code></a>: borrow&lt;<a href="#incoming_body"><a href="#incoming_body"><code>incoming-body</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_incoming_body.stream.0"></a> result&lt;own&lt;<a href="#input_stream"><a href="#input_stream"><code>input-stream</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="static_incoming_body.finish"><code>[static]incoming-body.finish: func</code></a></h4>
<p>Takes ownership of <a href="#incoming_body"><code>incoming-body</code></a>, and returns a <a href="#future_trailers"><code>future-trailers</code></a>.
This function will trap if the <a href="#input_stream"><code>input-stream</code></a> child is still alive.</p>
<h5>Params</h5>
<ul>
<li><a name="static_incoming_body.finish.this"><code>this</code></a>: own&lt;<a href="#incoming_body"><a href="#incoming_body"><code>incoming-body</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="static_incoming_body.finish.0"></a> own&lt;<a href="#future_trailers"><a href="#future_trailers"><code>future-trailers</code></a></a>&gt;</li>
</ul>
<h4><a name="method_future_trailers.subscribe"><code>[method]future-trailers.subscribe: func</code></a></h4>
<p>Returns a pollable which becomes ready when either the trailers have
been received, or an error has occured. When this pollable is ready,
the <a href="#get"><code>get</code></a> method will return <code>some</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_future_trailers.subscribe.self"><code>self</code></a>: borrow&lt;<a href="#future_trailers"><a href="#future_trailers"><code>future-trailers</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_future_trailers.subscribe.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="method_future_trailers.get"><code>[method]future-trailers.get: func</code></a></h4>
<p>Returns the contents of the trailers, or an error which occured,
once the future is ready.</p>
<p>The outer <code>option</code> represents future readiness. Users can wait on this
<code>option</code> to become <code>some</code> using the <code>subscribe</code> method.</p>
<p>The outer <code>result</code> is used to retrieve the trailers or error at most
once. It will be success on the first call in which the outer option
is <code>some</code>, and error on subsequent calls.</p>
<p>The inner <code>result</code> represents that either the HTTP Request or Response
body, as well as any trailers, were received successfully, or that an
error occured receiving them. The optional <a href="#trailers"><code>trailers</code></a> indicates whether
or not trailers were present in the body.</p>
<p>When some <a href="#trailers"><code>trailers</code></a> are returned by this method, the <a href="#trailers"><code>trailers</code></a>
resource is immutable, and a child. Use of the <a href="#set"><code>set</code></a>, <code>append</code>, or
<code>delete</code> methods will return an error, and the resource must be
dropped before the parent <a href="#future_trailers"><code>future-trailers</code></a> is dropped.</p>
<h5>Params</h5>
<ul>
<li><a name="method_future_trailers.get.self"><code>self</code></a>: borrow&lt;<a href="#future_trailers"><a href="#future_trailers"><code>future-trailers</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_future_trailers.get.0"></a> option&lt;result&lt;result&lt;option&lt;own&lt;<a href="#trailers"><a href="#trailers"><code>trailers</code></a></a>&gt;&gt;, <a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;&gt;&gt;</li>
</ul>
<h4><a name="constructor_outgoing_response"><code>[constructor]outgoing-response: func</code></a></h4>
<p>Construct an <a href="#outgoing_response"><code>outgoing-response</code></a>, with a default <a href="#status_code"><code>status-code</code></a> of <code>200</code>.
If a different <a href="#status_code"><code>status-code</code></a> is needed, it must be set via the
<code>set-status-code</code> method.</p>
<ul>
<li><a href="#headers"><code>headers</code></a> is the HTTP Headers for the Response.</li>
</ul>
<h5>Params</h5>
<ul>
<li><a name="constructor_outgoing_response.headers"><a href="#headers"><code>headers</code></a></a>: own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="constructor_outgoing_response.0"></a> own&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;</li>
</ul>
<h4><a name="method_outgoing_response.status_code"><code>[method]outgoing-response.status-code: func</code></a></h4>
<p>Get the HTTP Status Code for the Response.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_response.status_code.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_response.status_code.0"></a> <a href="#status_code"><a href="#status_code"><code>status-code</code></a></a></li>
</ul>
<h4><a name="method_outgoing_response.set_status_code"><code>[method]outgoing-response.set-status-code: func</code></a></h4>
<p>Set the HTTP Status Code for the Response. Fails if the status-code
given is not a valid http status code.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_response.set_status_code.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;</li>
<li><a name="method_outgoing_response.set_status_code.status_code"><a href="#status_code"><code>status-code</code></a></a>: <a href="#status_code"><a href="#status_code"><code>status-code</code></a></a></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_response.set_status_code.0"></a> result</li>
</ul>
<h4><a name="method_outgoing_response.headers"><code>[method]outgoing-response.headers: func</code></a></h4>
<p>Get the headers associated with the Request.</p>
<p>The returned <a href="#headers"><code>headers</code></a> resource is immutable: <a href="#set"><code>set</code></a>, <code>append</code>, and
<code>delete</code> operations will fail with <code>header-error.immutable</code>.</p>
<p>This headers resource is a child: it must be dropped before the parent
<a href="#outgoing_request"><code>outgoing-request</code></a> is dropped, or its ownership is transfered to
another component by e.g. <code>outgoing-handler.handle</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_response.headers.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_response.headers.0"></a> own&lt;<a href="#headers"><a href="#headers"><code>headers</code></a></a>&gt;</li>
</ul>
<h4><a name="method_outgoing_response.body"><code>[method]outgoing-response.body: func</code></a></h4>
<p>Returns the resource corresponding to the outgoing Body for this Response.</p>
<p>Returns success on the first call: the <a href="#outgoing_body"><code>outgoing-body</code></a> resource for
this <a href="#outgoing_response"><code>outgoing-response</code></a> can be retrieved at most once. Subsequent
calls will return error.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_response.body.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_response"><a href="#outgoing_response"><code>outgoing-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_response.body.0"></a> result&lt;own&lt;<a href="#outgoing_body"><a href="#outgoing_body"><code>outgoing-body</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="method_outgoing_body.write"><code>[method]outgoing-body.write: func</code></a></h4>
<p>Returns a stream for writing the body contents.</p>
<p>The returned <a href="#output_stream"><code>output-stream</code></a> is a child resource: it must be dropped
before the parent <a href="#outgoing_body"><code>outgoing-body</code></a> resource is dropped (or finished),
otherwise the <a href="#outgoing_body"><code>outgoing-body</code></a> drop or <code>finish</code> will trap.</p>
<p>Returns success on the first call: the <a href="#output_stream"><code>output-stream</code></a> resource for
this <a href="#outgoing_body"><code>outgoing-body</code></a> may be retrieved at most once. Subsequent calls
will return error.</p>
<h5>Params</h5>
<ul>
<li><a name="method_outgoing_body.write.self"><code>self</code></a>: borrow&lt;<a href="#outgoing_body"><a href="#outgoing_body"><code>outgoing-body</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_outgoing_body.write.0"></a> result&lt;own&lt;<a href="#output_stream"><a href="#output_stream"><code>output-stream</code></a></a>&gt;&gt;</li>
</ul>
<h4><a name="static_outgoing_body.finish"><code>[static]outgoing-body.finish: func</code></a></h4>
<p>Finalize an outgoing body, optionally providing trailers. This must be
called to signal that the response is complete. If the <a href="#outgoing_body"><code>outgoing-body</code></a>
is dropped without calling <code>outgoing-body.finalize</code>, the implementation
should treat the body as corrupted.</p>
<p>Fails if the body's <a href="#outgoing_request"><code>outgoing-request</code></a> or <a href="#outgoing_response"><code>outgoing-response</code></a> was
constructed with a Content-Length header, and the contents written
to the body (via <code>write</code>) does not match the value given in the
Content-Length.</p>
<h5>Params</h5>
<ul>
<li><a name="static_outgoing_body.finish.this"><code>this</code></a>: own&lt;<a href="#outgoing_body"><a href="#outgoing_body"><code>outgoing-body</code></a></a>&gt;</li>
<li><a name="static_outgoing_body.finish.trailers"><a href="#trailers"><code>trailers</code></a></a>: option&lt;own&lt;<a href="#trailers"><a href="#trailers"><code>trailers</code></a></a>&gt;&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="static_outgoing_body.finish.0"></a> result&lt;_, <a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;</li>
</ul>
<h4><a name="method_future_incoming_response.subscribe"><code>[method]future-incoming-response.subscribe: func</code></a></h4>
<p>Returns a pollable which becomes ready when either the Response has
been received, or an error has occured. When this pollable is ready,
the <a href="#get"><code>get</code></a> method will return <code>some</code>.</p>
<h5>Params</h5>
<ul>
<li><a name="method_future_incoming_response.subscribe.self"><code>self</code></a>: borrow&lt;<a href="#future_incoming_response"><a href="#future_incoming_response"><code>future-incoming-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_future_incoming_response.subscribe.0"></a> own&lt;<a href="#pollable"><a href="#pollable"><code>pollable</code></a></a>&gt;</li>
</ul>
<h4><a name="method_future_incoming_response.get"><code>[method]future-incoming-response.get: func</code></a></h4>
<p>Returns the incoming HTTP Response, or an error, once one is ready.</p>
<p>The outer <code>option</code> represents future readiness. Users can wait on this
<code>option</code> to become <code>some</code> using the <code>subscribe</code> method.</p>
<p>The outer <code>result</code> is used to retrieve the response or error at most
once. It will be success on the first call in which the outer option
is <code>some</code>, and error on subsequent calls.</p>
<p>The inner <code>result</code> represents that either the incoming HTTP Response
status and headers have recieved successfully, or that an error
occured. Errors may also occur while consuming the response body,
but those will be reported by the <a href="#incoming_body"><code>incoming-body</code></a> and its
<a href="#output_stream"><code>output-stream</code></a> child.</p>
<h5>Params</h5>
<ul>
<li><a name="method_future_incoming_response.get.self"><code>self</code></a>: borrow&lt;<a href="#future_incoming_response"><a href="#future_incoming_response"><code>future-incoming-response</code></a></a>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="method_future_incoming_response.get.0"></a> option&lt;result&lt;result&lt;own&lt;<a href="#incoming_response"><a href="#incoming_response"><code>incoming-response</code></a></a>&gt;, <a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;&gt;&gt;</li>
</ul>
<h2><a name="wasi:http_outgoing_handler_0.2.0">Import interface wasi:http/outgoing-handler@0.2.0</a></h2>
<p>This interface defines a handler of outgoing HTTP Requests. It should be
imported by components which wish to make HTTP Requests.</p>
<hr />
<h3>Types</h3>
<h4><a name="outgoing_request"><code>type outgoing-request</code></a></h4>
<p><a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a></p>
<p>
#### <a name="request_options">`type request-options`</a>
[`request-options`](#request_options)
<p>
#### <a name="future_incoming_response">`type future-incoming-response`</a>
[`future-incoming-response`](#future_incoming_response)
<p>
#### <a name="error_code">`type error-code`</a>
[`error-code`](#error_code)
<p>
----
<h3>Functions</h3>
<h4><a name="handle"><code>handle: func</code></a></h4>
<p>This function is invoked with an outgoing HTTP Request, and it returns
a resource <a href="#future_incoming_response"><code>future-incoming-response</code></a> which represents an HTTP Response
which may arrive in the future.</p>
<p>The <code>options</code> argument accepts optional parameters for the HTTP
protocol's transport layer.</p>
<p>This function may return an error if the <a href="#outgoing_request"><code>outgoing-request</code></a> is invalid
or not allowed to be made. Otherwise, protocol errors are reported
through the <a href="#future_incoming_response"><code>future-incoming-response</code></a>.</p>
<h5>Params</h5>
<ul>
<li><a name="handle.request"><code>request</code></a>: own&lt;<a href="#outgoing_request"><a href="#outgoing_request"><code>outgoing-request</code></a></a>&gt;</li>
<li><a name="handle.options"><code>options</code></a>: option&lt;own&lt;<a href="#request_options"><a href="#request_options"><code>request-options</code></a></a>&gt;&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="handle.0"></a> result&lt;own&lt;<a href="#future_incoming_response"><a href="#future_incoming_response"><code>future-incoming-response</code></a></a>&gt;, <a href="#error_code"><a href="#error_code"><code>error-code</code></a></a>&gt;</li>
</ul>
<h2><a name="bulwark:plugin_types_0.5.0">Import interface bulwark:plugin/types@0.5.0</a></h2>
<hr />
<h3>Types</h3>
<h4><a name="decision"><code>record decision</code></a></h4>
<p>A <code>Decision</code> represents evidence in favor of either accepting or restricting an operation under consideration.</p>
<p>It is composed of three values: <code>accept</code>, <code>restrict</code> and <code>unknown</code>. Each must be between 0.0 and 1.0 inclusive
and the sum of all three must equal 1.0. The <code>unknown</code> value represents uncertainty about the evidence, with
a 1.0 <code>unknown</code> value indicating total uncertainty or a &quot;no opinion&quot; verdict. Similarly, a 1.0 <code>accept</code> or
<code>restrict</code> value indicates total certainty that the verdict should be to accept or to restrict, respectively.</p>
<p>This representation allows for a fairly intuitive way of characterizing evidence in favor of or against
blocking an operation, while still capturing any uncertainty. Limiting to two states rather than a wider range of
classification possibilities allows for better performance optimizations, simplifies code readability, and
enables useful transformations like reweighting a <code>Decision</code>.</p>
<p>This data structure is a two-state <a href="https://en.wikipedia.org/wiki/Dempster%E2%80%93Shafer_theory">Dempster-Shafer</a>
mass function, with the power set represented by the <code>unknown</code> value. This enables the use of combination rules
to aggregate decisions from multiple sources. However, knowledge of Dempster-Shafer theory should not be necessary.</p>
<h5>Record Fields</h5>
<ul>
<li>
<p><a name="decision.accepted"><code>accepted</code></a>: <code>float64</code></p>
<p>The `accepted` value represents evidence in favor of accepting the operation.
</li>
<li>
<p><a name="decision.restricted"><code>restricted</code></a>: <code>float64</code></p>
<p>The `restricted` value represents evidence in favor of restricting the operation.
</li>
<li>
<p><a name="decision.unknown"><code>unknown</code></a>: <code>float64</code></p>
<p>The `unknown` value represents uncertainty about whether to accept or restrict the operation.
</li>
</ul>
<h4><a name="outcome"><code>enum outcome</code></a></h4>
<p>An <code>Outcome</code> represents a verdict after it's been compared to configured decision thresholds.</p>
<h5>Enum Cases</h5>
<ul>
<li>
<p><a name="outcome.restricted"><code>restricted</code></a></p>
<p>The `restricted` outcome indicates that the operation should be restricted, usually by blocking the request.
</li>
<li>
<p><a name="outcome.suspected"><code>suspected</code></a></p>
<p>The `suspected` outcome indicates that the operation may not be safe, but no action was taken.
</li>
<li>
<p><a name="outcome.accepted"><code>accepted</code></a></p>
<p>The `accepted` outcome indicates that the operation is likely safe and the request will be allowed.
</li>
<li>
<p><a name="outcome.trusted"><code>trusted</code></a></p>
<p>The `trusted` outcome indicates that the operation is very likely safe and the request will be allowed.
</li>
</ul>
<h4><a name="verdict"><code>record verdict</code></a></h4>
<p>A <code>Verdict</code> represents a combined decision across multiple detections.</p>
<h5>Record Fields</h5>
<ul>
<li>
<p><a name="verdict.decision"><a href="#decision"><code>decision</code></a></a>: <a href="#decision"><a href="#decision"><code>decision</code></a></a></p>
<p>The `decision` value represents the combined numerical decision from multiple detections.
</li>
<li>
<p><a name="verdict.outcome"><a href="#outcome"><code>outcome</code></a></a>: <a href="#outcome"><a href="#outcome"><code>outcome</code></a></a></p>
<p>The `outcome` value represents a comparison of the numerical decision against a set of thresholds.
</li>
<li>
<p><a name="verdict.tags"><code>tags</code></a>: list&lt;<code>string</code>&gt;</p>
<p>The `tags` value represents the merged tags used to annotate the request.
</li>
</ul>
<h4><a name="param"><code>tuple param</code></a></h4>
<p>A <code>Param</code> maps a parameter name to a parameter value.</p>
<p>Parameters are used to represent arbitrary information about a request. They may be application-specific and are
often produced by parsing the request, decrypting session cookies, or by calling out to an external service.</p>
<h5>Tuple Fields</h5>
<ul>
<li><a name="param.0"><code>0</code></a>: <code>string</code></li>
<li><a name="param.1"><code>1</code></a>: <code>string</code></li>
</ul>
<h4><a name="handler_output"><code>record handler-output</code></a></h4>
<p>A <code>HandlerOutput</code> represents the combined result of executing a detection's handlers.</p>
<h5>Record Fields</h5>
<ul>
<li>
<p><a name="handler_output.params"><code>params</code></a>: list&lt;<a href="#param"><a href="#param"><code>param</code></a></a>&gt;</p>
<p>The `params` value represents parameters used to enrich the request with additional information.
</li>
<li>
<p><a name="handler_output.decision"><a href="#decision"><code>decision</code></a></a>: <a href="#decision"><a href="#decision"><code>decision</code></a></a></p>
<p>The `decision` value represents the verdict of the handler.
</li>
<li>
<p><a name="handler_output.tags"><code>tags</code></a>: list&lt;<code>string</code>&gt;</p>
<p>The `tags` value represents tags used to annotate the request.
</li>
</ul>
<h2><a name="bulwark:plugin_config_0.5.0">Import interface bulwark:plugin/config@0.5.0</a></h2>
<hr />
<h3>Types</h3>
<h4><a name="number"><code>variant number</code></a></h4>
<h5>Variant Cases</h5>
<ul>
<li>
<p><a name="number.posint"><code>posint</code></a>: <code>u64</code></p>
</li>
<li>
<p><a name="number.negint"><code>negint</code></a>: <code>s64</code></p>
<p>Always less than zero.
</li>
<li>
<p><a name="number.float"><code>float</code></a>: <code>float64</code></p>
<p>Always finite.
</li>
</ul>
<h4><a name="primitive_value"><code>variant primitive-value</code></a></h4>
<p>Types cannot be recursive in WIT, so we need to create a
second type for the top-level type to reference.</p>
<h5>Variant Cases</h5>
<ul>
<li><a name="primitive_value.null"><code>null</code></a></li>
<li><a name="primitive_value.boolean"><code>boolean</code></a>: <code>bool</code></li>
<li><a name="primitive_value.num"><code>num</code></a>: <a href="#number"><a href="#number"><code>number</code></a></a></li>
<li><a name="primitive_value.str"><code>str</code></a>: <code>string</code></li>
</ul>
<h4><a name="value"><code>variant value</code></a></h4>
<h5>Variant Cases</h5>
<ul>
<li><a name="value.null"><code>null</code></a></li>
<li><a name="value.boolean"><code>boolean</code></a>: <code>bool</code></li>
<li><a name="value.num"><code>num</code></a>: <a href="#number"><a href="#number"><code>number</code></a></a></li>
<li><a name="value.str"><code>str</code></a>: <code>string</code></li>
<li><a name="value.arr"><code>arr</code></a>: list&lt;<a href="#primitive_value"><a href="#primitive_value"><code>primitive-value</code></a></a>&gt;</li>
<li><a name="value.obj"><code>obj</code></a>: list&lt;(<code>string</code>, <a href="#primitive_value"><a href="#primitive_value"><code>primitive-value</code></a></a>)&gt;<p>There's no native map type in WIT, so we use a list of tuples.
</li>
</ul>
<h4><a name="error"><code>variant error</code></a></h4>
<h5>Variant Cases</h5>
<ul>
<li><a name="error.missing"><code>missing</code></a>: <code>string</code></li>
<li><a name="error.invalid_serialization"><code>invalid-serialization</code></a>: <code>string</code></li>
<li><a name="error.invalid_unicode"><code>invalid-unicode</code></a>: <code>string</code></li>
<li><a name="error.invalid_conversion"><code>invalid-conversion</code></a>: <code>string</code></li>
<li><a name="error.permission"><code>permission</code></a>: <code>string</code></li>
</ul>
<hr />
<h3>Functions</h3>
<h4><a name="config_keys"><code>config-keys: func</code></a></h4>
<p>Returns all config key names.</p>
<h5>Return values</h5>
<ul>
<li><a name="config_keys.0"></a> list&lt;<code>string</code>&gt;</li>
</ul>
<h4><a name="config_var"><code>config-var: func</code></a></h4>
<p>Returns the named config value.</p>
<h5>Params</h5>
<ul>
<li><a name="config_var.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="config_var.0"></a> result&lt;option&lt;<a href="#value"><a href="#value"><code>value</code></a></a>&gt;, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="proxy_hops"><code>proxy-hops: func</code></a></h4>
<p>Returns the number of proxy hops expected exterior to Bulwark.</p>
<h5>Return values</h5>
<ul>
<li><a name="proxy_hops.0"></a> <code>u8</code></li>
</ul>
<h2><a name="bulwark:plugin_redis_0.5.0">Import interface bulwark:plugin/redis@0.5.0</a></h2>
<hr />
<h3>Types</h3>
<h4><a name="rate"><code>record rate</code></a></h4>
<h5>Record Fields</h5>
<ul>
<li>
<p><a name="rate.attempts"><code>attempts</code></a>: <code>s64</code></p>
<p>The number of operations that have been attempted with the corresponding key.
</li>
<li>
<p><a name="rate.expiration"><code>expiration</code></a>: <code>s64</code></p>
<p>The unix timestamp corresponding to when this key will no longer be active.
<p>This value is managed by Bulwark rather than Redis for precision.</p>
</li>
</ul>
<h4><a name="breaker"><code>record breaker</code></a></h4>
<h5>Record Fields</h5>
<ul>
<li>
<p><a name="breaker.generation"><code>generation</code></a>: <code>s64</code></p>
<p>The number of breaker increments that have been attempted with the corresponding key.
<p>This value will always increment by one, regardless of whether successes or failures
are being incremented by a delta larger than one.</p>
</li>
<li>
<p><a name="breaker.successes"><code>successes</code></a>: <code>s64</code></p>
<p>The number of successes that have occurred with the corresponding key.
</li>
<li>
<p><a name="breaker.failures"><code>failures</code></a>: <code>s64</code></p>
<p>The number of failures that have occurred with the corresponding key.
</li>
<li>
<p><a name="breaker.consecutive_successes"><code>consecutive-successes</code></a>: <code>s64</code></p>
<p>The number of consecutive successes that have occurred with the corresponding key.
</li>
<li>
<p><a name="breaker.consecutive_failures"><code>consecutive-failures</code></a>: <code>s64</code></p>
<p>The number of consecutive failures that have occurred with the corresponding key.
</li>
<li>
<p><a name="breaker.expiration"><code>expiration</code></a>: <code>s64</code></p>
<p>The unix timestamp corresponding to when this key will no longer be active.
<p>This value is managed by Bulwark rather than Redis for precision.</p>
</li>
</ul>
<h4><a name="value"><code>type value</code></a></h4>
<p><a href="#value"><a href="#value"><code>value</code></a></a></p>
<p>The value being stored or retrieved.
<h4><a name="error"><code>variant error</code></a></h4>
<p>Errors related to interacting with Redis</p>
<h5>Variant Cases</h5>
<ul>
<li>
<p><a name="error.permission"><code>permission</code></a>: <code>string</code></p>
<p>The plugin did not have permission to access this key prefix.
</li>
<li>
<p><a name="error.remote"><code>remote</code></a>: <code>string</code></p>
<p>There was an error communicating with Redis.
</li>
<li>
<p><a name="error.type_error"><code>type-error</code></a></p>
<p>There was a type mismatch.
</li>
<li>
<p><a name="error.other"><code>other</code></a>: <code>string</code></p>
<p>Some other error occurred.
</li>
</ul>
<hr />
<h3>Functions</h3>
<h4><a name="get"><code>get: func</code></a></h4>
<p>Retrieves the value associated with the given key.</p>
<h5>Params</h5>
<ul>
<li><a name="get.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="get.0"></a> result&lt;option&lt;list&lt;<code>u8</code>&gt;&gt;, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="set"><code>set: func</code></a></h4>
<p>Sets the given key to the given value.</p>
<p>Overwrites any previously existing value.</p>
<h5>Params</h5>
<ul>
<li><a name="set.key"><code>key</code></a>: <code>string</code></li>
<li><a name="set.value"><a href="#value"><code>value</code></a></a>: list&lt;<code>u8</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="set.0"></a> result&lt;_, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="del"><code>del: func</code></a></h4>
<p>Removes the given keys.</p>
<p>Non-existant keys are ignored. Returns the number of keys that were removed.</p>
<h5>Params</h5>
<ul>
<li><a name="del.keys"><code>keys</code></a>: list&lt;<code>string</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="del.0"></a> result&lt;<code>u32</code>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="incr"><code>incr: func</code></a></h4>
<p>Increments the value associated with the given key by one.</p>
<p>If the key does not exist, it is set to zero before being incremented.
If the key already has a value that cannot be incremented, a <a href="#error.type_error"><code>error::type-error</code></a> is returned.</p>
<h5>Params</h5>
<ul>
<li><a name="incr.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="incr.0"></a> result&lt;<code>s64</code>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="incr_by"><code>incr-by: func</code></a></h4>
<p>Increments the value associated with the given key by the given delta.</p>
<p>If the key does not exist, it is set to zero before being incremented.
If the key already has a value that cannot be incremented, a <a href="#error.type_error"><code>error::type-error</code></a> is returned.</p>
<h5>Params</h5>
<ul>
<li><a name="incr_by.key"><code>key</code></a>: <code>string</code></li>
<li><a name="incr_by.delta"><code>delta</code></a>: <code>s64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="incr_by.0"></a> result&lt;<code>s64</code>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="sadd"><code>sadd: func</code></a></h4>
<p>Adds the given values to the named set.</p>
<p>Returns the number of elements that were added to the set,
not including all the elements already present in the set.</p>
<h5>Params</h5>
<ul>
<li><a name="sadd.key"><code>key</code></a>: <code>string</code></li>
<li><a name="sadd.values"><code>values</code></a>: list&lt;<code>string</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="sadd.0"></a> result&lt;<code>u32</code>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="smembers"><code>smembers: func</code></a></h4>
<p>Returns the contents of the given set.</p>
<h5>Params</h5>
<ul>
<li><a name="smembers.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="smembers.0"></a> result&lt;list&lt;<code>string</code>&gt;, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="srem"><code>srem: func</code></a></h4>
<p>Removes the given values from the named set.</p>
<p>Returns the number of members that were removed from the set,
not including non existing members.</p>
<h5>Params</h5>
<ul>
<li><a name="srem.key"><code>key</code></a>: <code>string</code></li>
<li><a name="srem.values"><code>values</code></a>: list&lt;<code>string</code>&gt;</li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="srem.0"></a> result&lt;<code>u32</code>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="expire"><code>expire: func</code></a></h4>
<p>Sets the time to live for the given key.</p>
<h5>Params</h5>
<ul>
<li><a name="expire.key"><code>key</code></a>: <code>string</code></li>
<li><a name="expire.ttl"><code>ttl</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="expire.0"></a> result&lt;_, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="expire_at"><code>expire-at: func</code></a></h4>
<p>Sets the expiration for the given key to the given unix time.</p>
<h5>Params</h5>
<ul>
<li><a name="expire_at.key"><code>key</code></a>: <code>string</code></li>
<li><a name="expire_at.unix_time"><code>unix-time</code></a>: <code>u64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="expire_at.0"></a> result&lt;_, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="incr_rate_limit"><code>incr-rate-limit: func</code></a></h4>
<h5>Params</h5>
<ul>
<li><a name="incr_rate_limit.key"><code>key</code></a>: <code>string</code></li>
<li><a name="incr_rate_limit.delta"><code>delta</code></a>: <code>s64</code></li>
<li><a name="incr_rate_limit.window"><code>window</code></a>: <code>s64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="incr_rate_limit.0"></a> result&lt;<a href="#rate"><a href="#rate"><code>rate</code></a></a>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="check_rate_limit"><code>check-rate-limit: func</code></a></h4>
<h5>Params</h5>
<ul>
<li><a name="check_rate_limit.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="check_rate_limit.0"></a> result&lt;<a href="#rate"><a href="#rate"><code>rate</code></a></a>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="incr_breaker"><code>incr-breaker: func</code></a></h4>
<h5>Params</h5>
<ul>
<li><a name="incr_breaker.key"><code>key</code></a>: <code>string</code></li>
<li><a name="incr_breaker.success_delta"><code>success-delta</code></a>: <code>s64</code></li>
<li><a name="incr_breaker.failure_delta"><code>failure-delta</code></a>: <code>s64</code></li>
<li><a name="incr_breaker.window"><code>window</code></a>: <code>s64</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="incr_breaker.0"></a> result&lt;<a href="#breaker"><a href="#breaker"><code>breaker</code></a></a>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
<h4><a name="check_breaker"><code>check-breaker: func</code></a></h4>
<h5>Params</h5>
<ul>
<li><a name="check_breaker.key"><code>key</code></a>: <code>string</code></li>
</ul>
<h5>Return values</h5>
<ul>
<li><a name="check_breaker.0"></a> result&lt;<a href="#breaker"><a href="#breaker"><code>breaker</code></a></a>, <a href="#error"><a href="#error"><code>error</code></a></a>&gt;</li>
</ul>
