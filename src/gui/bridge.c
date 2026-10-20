#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#define CloseWindow Win32_CloseWindow
#define ShowCursor Win32_ShowCursor
#define Rectangle Win32_Rectangle
#define DrawText Win32_DrawText
#define DrawTextEx Win32_DrawTextEx
#define LoadImage Win32_LoadImage
#include <windows.h>
#undef CloseWindow
#undef ShowCursor
#undef Rectangle
#undef DrawText
#undef DrawTextEx
#undef LoadImage
static SRWLOCK ultimate_events_lock = SRWLOCK_INIT;
static void Ultimate_LockEvents(void) { AcquireSRWLockExclusive(&ultimate_events_lock); }
static void Ultimate_UnlockEvents(void) { ReleaseSRWLockExclusive(&ultimate_events_lock); }
#else
#include <pthread.h>
static pthread_mutex_t ultimate_events_lock = PTHREAD_MUTEX_INITIALIZER;
static void Ultimate_LockEvents(void) { pthread_mutex_lock(&ultimate_events_lock); }
static void Ultimate_UnlockEvents(void) { pthread_mutex_unlock(&ultimate_events_lock); }
#endif

#define CLAY_IMPLEMENTATION
#include "bridge.h"
#define GLFW_INCLUDE_NONE
#include <GLFW/glfw3.h>

// Notifications are coalesced, and cannot race GLFW initialization/teardown.
static bool ultimate_events_ready = false;
static bool ultimate_wake_pending = false;
static bool ultimate_char_pending = false;
static GLFWcharfun ultimate_previous_char_callback;
static void Ultimate_CharCallback(GLFWwindow *window, unsigned int codepoint) {
    ultimate_char_pending = true;
    if (ultimate_previous_char_callback) ultimate_previous_char_callback(window, codepoint);
}

// Camera2D scales geometry, but OpenGL scissor rectangles bypass the camera.
static float ultimate_scale = 1.0f;
static void Ultimate_Scissor(int x, int y, int width, int height) {
    BeginScissorMode((int)(x * ultimate_scale), (int)(y * ultimate_scale),
                     (int)(width * ultimate_scale), (int)(height * ultimate_scale));
}
#define BeginScissorMode Ultimate_Scissor
#include "renderers/raylib/clay_renderer_raylib.c"
#undef BeginScissorMode

// Unlike the upstream ASCII-only measurement callback, this handles UTF-8 and
// respects the slice boundary. Rendering and measurement use the same glyphs.
static Clay_Dimensions Ultimate_Measure(Clay_StringSlice text, Clay_TextElementConfig *config, void *userData) {
    Font font = ((Font *)userData)[config->fontId];
    float scale = (float)config->fontSize / font.baseSize;
    float width = 0, line = 0;
    int lines = 1, count = 0;
    for (int i = 0; i < text.length;) {
        unsigned char first = (unsigned char)text.chars[i];
        int length = first < 0x80 ? 1 : first < 0xe0 ? 2 : first < 0xf0 ? 3 : 4;
        if (i + length > text.length) break;
        char bytes[5] = {0};
        memcpy(bytes, text.chars + i, length);
        int consumed;
        int codepoint = GetCodepointNext(bytes, &consumed);
        i += length;
        if (codepoint == '\n') {
            width = fmaxf(width, line);
            line = 0; count = 0; lines++;
            continue;
        }
        int index = GetGlyphIndex(font, codepoint);
        if (count++) line += config->letterSpacing;
        line += (font.glyphs[index].advanceX ? font.glyphs[index].advanceX : font.recs[index].width + font.glyphs[index].offsetX) * scale;
    }
    return (Clay_Dimensions){fmaxf(width, line), (float)config->fontSize * lines};
}
// raylib 5.5's JoystickCallback writes CORE.Input.Gamepad.name[jid] for any GLFW jid (0..15),
// but the array has MAX_GAMEPADS (4) rows: a fifth connected joystick (e.g. Wine/Steam virtual
// pads plus ours) wrote out of bounds and _FORTIFY_SOURCE aborted the GUI (live, twice).
// raylib only polls gamepads 0..3 anyway, so higher ids are dropped before reaching it.
#define ULTIMATE_RAYLIB_GAMEPADS 4
static GLFWjoystickfun ultimate_raylib_joystick_callback = NULL;
static void Ultimate_JoystickCallback(int jid, int event) {
    if (jid >= 0 && jid < ULTIMATE_RAYLIB_GAMEPADS && ultimate_raylib_joystick_callback)
        ultimate_raylib_joystick_callback(jid, event);
}
void Ultimate_Initialize(int width, int height, const char *title) {
    Ultimate_LockEvents();
    Clay_Raylib_Initialize(width, height, title, FLAG_WINDOW_RESIZABLE | FLAG_WINDOW_HIGHDPI | FLAG_VSYNC_HINT);
    // The stock renderer reuses this buffer. Reserve once, not during a frame.
    temp_render_buffer_len = 65537;
    temp_render_buffer = malloc(temp_render_buffer_len);
    if (IsWindowReady()) {
        ultimate_previous_char_callback = glfwSetCharCallback(glfwGetCurrentContext(), Ultimate_CharCallback);
        ultimate_raylib_joystick_callback = glfwSetJoystickCallback(Ultimate_JoystickCallback);
        ultimate_events_ready = true;
        ultimate_wake_pending = false;
    }
    Ultimate_UnlockEvents();
}
void Ultimate_MeasureFont(Font *fonts) { Clay_SetMeasureTextFunction(Ultimate_Measure, fonts); }
void Ultimate_Render(Clay_RenderCommandArray commands, Font *fonts, float scale) {
    // Raylib already converts scissor window coordinates to framebuffer pixels.
    // Remove that conversion from our camera scale to avoid applying DPI twice.
    ultimate_scale = scale * (float)GetScreenWidth() / (float)GetRenderWidth();
    Clay_Raylib_Render(commands, fonts);
}
void Ultimate_Close(void) {
    Ultimate_LockEvents();
    ultimate_events_ready = false;
    UnloadShader(overlayShader);
    Clay_Raylib_Close();
    Ultimate_UnlockEvents();
}

Ultimate_WindowMetrics Ultimate_GetWindowMetrics(void) {
    // Raylib GetWindowHandle is wl_surface*/XID on Linux, not GLFWwindow*.
    GLFWwindow *window = glfwGetCurrentContext();
    double x, y;
    int width, height, framebufferWidth, framebufferHeight;
    float scaleX, scaleY;
    glfwGetCursorPos(window, &x, &y);
    glfwGetWindowSize(window, &width, &height);
    glfwGetFramebufferSize(window, &framebufferWidth, &framebufferHeight);
    glfwGetWindowContentScale(window, &scaleX, &scaleY);
    return (Ultimate_WindowMetrics){
        .pointer = {(float)x, (float)y},
        .raylibPointer = GetMousePosition(),
        .window = {(float)width, (float)height},
        .framebuffer = {(float)framebufferWidth, (float)framebufferHeight},
        .dpi = {scaleX, scaleY},
        .highDpi = IsWindowState(FLAG_WINDOW_HIGHDPI)
    };
}

void Ultimate_WakeEvents(void) {
    Ultimate_LockEvents();
    if (ultimate_events_ready && !ultimate_wake_pending) {
        ultimate_wake_pending = true;
        glfwPostEmptyEvent();
    }
    Ultimate_UnlockEvents();
}

// Clear BEFORE the caller re-polls its mailbox. A later publication either
// changes that snapshot or leaves a pending native event that prevents sleep.
void Ultimate_PrepareWait(void) {
    Ultimate_LockEvents();
    ultimate_wake_pending = false;
    Ultimate_UnlockEvents();
}

bool Ultimate_PollInputEvents(void) {
    Vector2 before = GetMousePosition();
    ultimate_char_pending = false;
    PollInputEvents(); // reset old edges BEFORE dispatching fresh callbacks
    Vector2 after = GetMousePosition();
    Vector2 wheel = GetMouseWheelMoveV();
    if (ultimate_char_pending || before.x != after.x || before.y != after.y ||
        wheel.x != 0 || wheel.y != 0 || IsWindowResized() || WindowShouldClose()) return true;
    for (int key = 0; key <= GLFW_KEY_LAST; ++key)
        if (IsKeyPressed(key) || IsKeyReleased(key) || IsKeyPressedRepeat(key)) return true;
    for (int button = 0; button <= GLFW_MOUSE_BUTTON_LAST; ++button)
        if (IsMouseButtonPressed(button) || IsMouseButtonReleased(button)) return true;
    return false;
}

void Ultimate_WaitEventsTimeout(double timeout) {
    Ultimate_LockEvents();
    bool pending = ultimate_wake_pending;
    Ultimate_UnlockEvents();
    // Do not poll raylib again after this wait: callbacks update the already
    // reset edge state, which the next GUI iteration must consume unchanged.
    if (!pending) glfwWaitEventsTimeout(timeout);
}
