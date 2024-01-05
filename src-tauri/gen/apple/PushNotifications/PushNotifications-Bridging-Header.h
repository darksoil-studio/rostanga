//
//  Use this file to import your target's public headers that you would like to expose to Swift.
//
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
  const uint8_t *bytes;
  size_t len;
} RustByteSlice;

struct push_notification;

struct push_notification * modify_notification(RustByteSlice notification);

// Free a `named_data` instance returned by `named_data_new`.
void notification_destroy(struct push_notification *data);

RustByteSlice notification_title(const struct push_notification *data);
RustByteSlice notification_body(const struct push_notification *data);
