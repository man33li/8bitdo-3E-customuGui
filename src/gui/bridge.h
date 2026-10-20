#include "clay.h"
#include <raylib.h>
void Ultimate_Initialize(int width, int height, const char *title);
void Ultimate_Render(Clay_RenderCommandArray commands, Font *fonts, float scale);
void Ultimate_MeasureFont(Font *fonts);
void Ultimate_Close(void);
typedef struct Ultimate_WindowMetrics {
    Vector2 pointer;
    Vector2 raylibPointer;
    Vector2 window;
    Vector2 framebuffer;
    Vector2 dpi;
    bool highDpi;
} Ultimate_WindowMetrics;
Ultimate_WindowMetrics Ultimate_GetWindowMetrics(void);
void Ultimate_WakeEvents(void);
void Ultimate_PrepareWait(void);
bool Ultimate_PollInputEvents(void);
void Ultimate_WaitEventsTimeout(double timeout);
