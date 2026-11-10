# Contract: values holzi turns into the tool's type

Before the guard, holzi looks at each top-level argument whose property in the tool's input schema declares exactly
one `type`, and turns the value only when its meaning is unique:

| Schema type | Sent                                      | Becomes         |
| ----------- | ----------------------------------------- | --------------- |
| `boolean`   | `"true"`, `"false"` (any case, no spaces) | `true`, `false` |
| `integer`   | a string of digits with an optional `-`   | that integer    |
| `number`    | a decimal number as text (`"2.5"`, `"3"`) | that number     |

Everything else stays as sent: `"yes"`, `"1"` for a boolean, `"five"`, nested properties, properties without a single
declared type. The turned arguments are what the guard checks, the permission dialog shows, the tool receives and the
chat stores. A turned call is not a failure and adds no message for the model.
