// node_modules/drizzle-orm/entity.js
var entityKind = /* @__PURE__ */ Symbol.for("drizzle:entityKind");
function is(value, type) {
  if (!value || typeof value !== "object") {
    return false;
  }
  if (value instanceof type) {
    return true;
  }
  if (!Object.prototype.hasOwnProperty.call(type, entityKind)) {
    throw new Error(
      `Class "${type.name ?? "<unknown>"}" doesn't look like a Drizzle entity. If this is incorrect and the class is provided by Drizzle, please report this as a bug.`
    );
  }
  let cls = Object.getPrototypeOf(value).constructor;
  if (cls) {
    while (cls) {
      if (entityKind in cls && cls[entityKind] === type[entityKind]) {
        return true;
      }
      cls = Object.getPrototypeOf(cls);
    }
  }
  return false;
}

// node_modules/drizzle-orm/logger.js
var ConsoleLogWriter = class {
  static [entityKind] = "ConsoleLogWriter";
  write(message) {
    console.log(message);
  }
};
var DefaultLogger = class {
  static [entityKind] = "DefaultLogger";
  writer;
  constructor(config) {
    this.writer = config?.writer ?? new ConsoleLogWriter();
  }
  logQuery(query, params) {
    const stringifiedParams = params.map((p) => {
      try {
        return JSON.stringify(p);
      } catch {
        return String(p);
      }
    });
    const paramsStr = stringifiedParams.length ? ` -- params: [${stringifiedParams.join(", ")}]` : "";
    this.writer.write(`Query: ${query}${paramsStr}`);
  }
};
var NoopLogger = class {
  static [entityKind] = "NoopLogger";
  logQuery() {
  }
};

// node_modules/drizzle-orm/table.utils.js
var TableName = /* @__PURE__ */ Symbol.for("drizzle:Name");

// node_modules/drizzle-orm/table.js
var Schema = /* @__PURE__ */ Symbol.for("drizzle:Schema");
var Columns = /* @__PURE__ */ Symbol.for("drizzle:Columns");
var ExtraConfigColumns = /* @__PURE__ */ Symbol.for("drizzle:ExtraConfigColumns");
var OriginalName = /* @__PURE__ */ Symbol.for("drizzle:OriginalName");
var BaseName = /* @__PURE__ */ Symbol.for("drizzle:BaseName");
var IsAlias = /* @__PURE__ */ Symbol.for("drizzle:IsAlias");
var ExtraConfigBuilder = /* @__PURE__ */ Symbol.for("drizzle:ExtraConfigBuilder");
var IsDrizzleTable = /* @__PURE__ */ Symbol.for("drizzle:IsDrizzleTable");
var Table = class {
  static [entityKind] = "Table";
  /** @internal */
  static Symbol = {
    Name: TableName,
    Schema,
    OriginalName,
    Columns,
    ExtraConfigColumns,
    BaseName,
    IsAlias,
    ExtraConfigBuilder
  };
  /**
   * @internal
   * Can be changed if the table is aliased.
   */
  [TableName];
  /**
   * @internal
   * Used to store the original name of the table, before any aliasing.
   */
  [OriginalName];
  /** @internal */
  [Schema];
  /** @internal */
  [Columns];
  /** @internal */
  [ExtraConfigColumns];
  /**
   *  @internal
   * Used to store the table name before the transformation via the `tableCreator` functions.
   */
  [BaseName];
  /** @internal */
  [IsAlias] = false;
  /** @internal */
  [IsDrizzleTable] = true;
  /** @internal */
  [ExtraConfigBuilder] = void 0;
  constructor(name, schema, baseName) {
    this[TableName] = this[OriginalName] = name;
    this[Schema] = schema;
    this[BaseName] = baseName;
  }
};
function getTableName(table2) {
  return table2[TableName];
}
function getTableUniqueName(table2) {
  return `${table2[Schema] ?? "public"}.${table2[TableName]}`;
}

// node_modules/drizzle-orm/column.js
var Column = class {
  constructor(table2, config) {
    this.table = table2;
    this.config = config;
    this.name = config.name;
    this.keyAsName = config.keyAsName;
    this.notNull = config.notNull;
    this.default = config.default;
    this.defaultFn = config.defaultFn;
    this.onUpdateFn = config.onUpdateFn;
    this.hasDefault = config.hasDefault;
    this.primary = config.primaryKey;
    this.isUnique = config.isUnique;
    this.uniqueName = config.uniqueName;
    this.uniqueType = config.uniqueType;
    this.dataType = config.dataType;
    this.columnType = config.columnType;
    this.generated = config.generated;
    this.generatedIdentity = config.generatedIdentity;
  }
  static [entityKind] = "Column";
  name;
  keyAsName;
  primary;
  notNull;
  default;
  defaultFn;
  onUpdateFn;
  hasDefault;
  isUnique;
  uniqueName;
  uniqueType;
  dataType;
  columnType;
  enumValues = void 0;
  generated = void 0;
  generatedIdentity = void 0;
  config;
  mapFromDriverValue(value) {
    return value;
  }
  mapToDriverValue(value) {
    return value;
  }
  // ** @internal */
  shouldDisableInsert() {
    return this.config.generated !== void 0 && this.config.generated.type !== "byDefault";
  }
};

// node_modules/drizzle-orm/column-builder.js
var ColumnBuilder = class {
  static [entityKind] = "ColumnBuilder";
  config;
  constructor(name, dataType, columnType) {
    this.config = {
      name,
      keyAsName: name === "",
      notNull: false,
      default: void 0,
      hasDefault: false,
      primaryKey: false,
      isUnique: false,
      uniqueName: void 0,
      uniqueType: void 0,
      dataType,
      columnType,
      generated: void 0
    };
  }
  /**
   * Changes the data type of the column. Commonly used with `json` columns. Also, useful for branded types.
   *
   * @example
   * ```ts
   * const users = pgTable('users', {
   * 	id: integer('id').$type<UserId>().primaryKey(),
   * 	details: json('details').$type<UserDetails>().notNull(),
   * });
   * ```
   */
  $type() {
    return this;
  }
  /**
   * Adds a `not null` clause to the column definition.
   *
   * Affects the `select` model of the table - columns *without* `not null` will be nullable on select.
   */
  notNull() {
    this.config.notNull = true;
    return this;
  }
  /**
   * Adds a `default <value>` clause to the column definition.
   *
   * Affects the `insert` model of the table - columns *with* `default` are optional on insert.
   *
   * If you need to set a dynamic default value, use {@link $defaultFn} instead.
   */
  default(value) {
    this.config.default = value;
    this.config.hasDefault = true;
    return this;
  }
  /**
   * Adds a dynamic default value to the column.
   * The function will be called when the row is inserted, and the returned value will be used as the column value.
   *
   * **Note:** This value does not affect the `drizzle-kit` behavior, it is only used at runtime in `drizzle-orm`.
   */
  $defaultFn(fn) {
    this.config.defaultFn = fn;
    this.config.hasDefault = true;
    return this;
  }
  /**
   * Alias for {@link $defaultFn}.
   */
  $default = this.$defaultFn;
  /**
   * Adds a dynamic update value to the column.
   * The function will be called when the row is updated, and the returned value will be used as the column value if none is provided.
   * If no `default` (or `$defaultFn`) value is provided, the function will be called when the row is inserted as well, and the returned value will be used as the column value.
   *
   * **Note:** This value does not affect the `drizzle-kit` behavior, it is only used at runtime in `drizzle-orm`.
   */
  $onUpdateFn(fn) {
    this.config.onUpdateFn = fn;
    this.config.hasDefault = true;
    return this;
  }
  /**
   * Alias for {@link $onUpdateFn}.
   */
  $onUpdate = this.$onUpdateFn;
  /**
   * Adds a `primary key` clause to the column definition. This implicitly makes the column `not null`.
   *
   * In SQLite, `integer primary key` implicitly makes the column auto-incrementing.
   */
  primaryKey() {
    this.config.primaryKey = true;
    this.config.notNull = true;
    return this;
  }
  /** @internal Sets the name of the column to the key within the table definition if a name was not given. */
  setName(name) {
    if (this.config.name !== "") return;
    this.config.name = name;
  }
};

// node_modules/drizzle-orm/pg-core/foreign-keys.js
var ForeignKeyBuilder = class {
  static [entityKind] = "PgForeignKeyBuilder";
  /** @internal */
  reference;
  /** @internal */
  _onUpdate = "no action";
  /** @internal */
  _onDelete = "no action";
  constructor(config, actions) {
    this.reference = () => {
      const { name, columns, foreignColumns } = config();
      return { name, columns, foreignTable: foreignColumns[0].table, foreignColumns };
    };
    if (actions) {
      this._onUpdate = actions.onUpdate;
      this._onDelete = actions.onDelete;
    }
  }
  onUpdate(action) {
    this._onUpdate = action === void 0 ? "no action" : action;
    return this;
  }
  onDelete(action) {
    this._onDelete = action === void 0 ? "no action" : action;
    return this;
  }
  /** @internal */
  build(table2) {
    return new ForeignKey(table2, this);
  }
};
var ForeignKey = class {
  constructor(table2, builder) {
    this.table = table2;
    this.reference = builder.reference;
    this.onUpdate = builder._onUpdate;
    this.onDelete = builder._onDelete;
  }
  static [entityKind] = "PgForeignKey";
  reference;
  onUpdate;
  onDelete;
  getName() {
    const { name, columns, foreignColumns } = this.reference();
    const columnNames = columns.map((column) => column.name);
    const foreignColumnNames = foreignColumns.map((column) => column.name);
    const chunks = [
      this.table[TableName],
      ...columnNames,
      foreignColumns[0].table[TableName],
      ...foreignColumnNames
    ];
    return name ?? `${chunks.join("_")}_fk`;
  }
};

// node_modules/drizzle-orm/tracing-utils.js
function iife(fn, ...args) {
  return fn(...args);
}

// node_modules/drizzle-orm/pg-core/unique-constraint.js
function uniqueKeyName(table2, columns) {
  return `${table2[TableName]}_${columns.join("_")}_unique`;
}
var UniqueConstraintBuilder = class {
  constructor(columns, name) {
    this.name = name;
    this.columns = columns;
  }
  static [entityKind] = "PgUniqueConstraintBuilder";
  /** @internal */
  columns;
  /** @internal */
  nullsNotDistinctConfig = false;
  nullsNotDistinct() {
    this.nullsNotDistinctConfig = true;
    return this;
  }
  /** @internal */
  build(table2) {
    return new UniqueConstraint(table2, this.columns, this.nullsNotDistinctConfig, this.name);
  }
};
var UniqueOnConstraintBuilder = class {
  static [entityKind] = "PgUniqueOnConstraintBuilder";
  /** @internal */
  name;
  constructor(name) {
    this.name = name;
  }
  on(...columns) {
    return new UniqueConstraintBuilder(columns, this.name);
  }
};
var UniqueConstraint = class {
  constructor(table2, columns, nullsNotDistinct, name) {
    this.table = table2;
    this.columns = columns;
    this.name = name ?? uniqueKeyName(this.table, this.columns.map((column) => column.name));
    this.nullsNotDistinct = nullsNotDistinct;
  }
  static [entityKind] = "PgUniqueConstraint";
  columns;
  name;
  nullsNotDistinct = false;
  getName() {
    return this.name;
  }
};

// node_modules/drizzle-orm/pg-core/utils/array.js
function parsePgArrayValue(arrayString, startFrom, inQuotes) {
  for (let i = startFrom; i < arrayString.length; i++) {
    const char = arrayString[i];
    if (char === "\\") {
      i++;
      continue;
    }
    if (char === '"') {
      return [arrayString.slice(startFrom, i).replace(/\\/g, ""), i + 1];
    }
    if (inQuotes) {
      continue;
    }
    if (char === "," || char === "}") {
      return [arrayString.slice(startFrom, i).replace(/\\/g, ""), i];
    }
  }
  return [arrayString.slice(startFrom).replace(/\\/g, ""), arrayString.length];
}
function parsePgNestedArray(arrayString, startFrom = 0) {
  const result = [];
  let i = startFrom;
  let lastCharIsComma = false;
  while (i < arrayString.length) {
    const char = arrayString[i];
    if (char === ",") {
      if (lastCharIsComma || i === startFrom) {
        result.push("");
      }
      lastCharIsComma = true;
      i++;
      continue;
    }
    lastCharIsComma = false;
    if (char === "\\") {
      i += 2;
      continue;
    }
    if (char === '"') {
      const [value2, startFrom2] = parsePgArrayValue(arrayString, i + 1, true);
      result.push(value2);
      i = startFrom2;
      continue;
    }
    if (char === "}") {
      return [result, i + 1];
    }
    if (char === "{") {
      const [value2, startFrom2] = parsePgNestedArray(arrayString, i + 1);
      result.push(value2);
      i = startFrom2;
      continue;
    }
    const [value, newStartFrom] = parsePgArrayValue(arrayString, i, false);
    result.push(value);
    i = newStartFrom;
  }
  return [result, i];
}
function parsePgArray(arrayString) {
  const [result] = parsePgNestedArray(arrayString, 1);
  return result;
}
function makePgArray(array) {
  return `{${array.map((item) => {
    if (Array.isArray(item)) {
      return makePgArray(item);
    }
    if (typeof item === "string") {
      return `"${item.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
    }
    return `${item}`;
  }).join(",")}}`;
}

// node_modules/drizzle-orm/pg-core/columns/common.js
var PgColumnBuilder = class extends ColumnBuilder {
  foreignKeyConfigs = [];
  static [entityKind] = "PgColumnBuilder";
  array(size) {
    return new PgArrayBuilder(this.config.name, this, size);
  }
  references(ref, actions = {}) {
    this.foreignKeyConfigs.push({ ref, actions });
    return this;
  }
  unique(name, config) {
    this.config.isUnique = true;
    this.config.uniqueName = name;
    this.config.uniqueType = config?.nulls;
    return this;
  }
  generatedAlwaysAs(as) {
    this.config.generated = {
      as,
      type: "always",
      mode: "stored"
    };
    return this;
  }
  /** @internal */
  buildForeignKeys(column, table2) {
    return this.foreignKeyConfigs.map(({ ref, actions }) => {
      return iife(
        (ref2, actions2) => {
          const builder = new ForeignKeyBuilder(() => {
            const foreignColumn = ref2();
            return { columns: [column], foreignColumns: [foreignColumn] };
          });
          if (actions2.onUpdate) {
            builder.onUpdate(actions2.onUpdate);
          }
          if (actions2.onDelete) {
            builder.onDelete(actions2.onDelete);
          }
          return builder.build(table2);
        },
        ref,
        actions
      );
    });
  }
  /** @internal */
  buildExtraConfigColumn(table2) {
    return new ExtraConfigColumn(table2, this.config);
  }
};
var PgColumn = class extends Column {
  constructor(table2, config) {
    if (!config.uniqueName) {
      config.uniqueName = uniqueKeyName(table2, [config.name]);
    }
    super(table2, config);
    this.table = table2;
  }
  static [entityKind] = "PgColumn";
};
var ExtraConfigColumn = class extends PgColumn {
  static [entityKind] = "ExtraConfigColumn";
  getSQLType() {
    return this.getSQLType();
  }
  indexConfig = {
    order: this.config.order ?? "asc",
    nulls: this.config.nulls ?? "last",
    opClass: this.config.opClass
  };
  defaultConfig = {
    order: "asc",
    nulls: "last",
    opClass: void 0
  };
  asc() {
    this.indexConfig.order = "asc";
    return this;
  }
  desc() {
    this.indexConfig.order = "desc";
    return this;
  }
  nullsFirst() {
    this.indexConfig.nulls = "first";
    return this;
  }
  nullsLast() {
    this.indexConfig.nulls = "last";
    return this;
  }
  /**
   * ### PostgreSQL documentation quote
   *
   * > An operator class with optional parameters can be specified for each column of an index.
   * The operator class identifies the operators to be used by the index for that column.
   * For example, a B-tree index on four-byte integers would use the int4_ops class;
   * this operator class includes comparison functions for four-byte integers.
   * In practice the default operator class for the column's data type is usually sufficient.
   * The main point of having operator classes is that for some data types, there could be more than one meaningful ordering.
   * For example, we might want to sort a complex-number data type either by absolute value or by real part.
   * We could do this by defining two operator classes for the data type and then selecting the proper class when creating an index.
   * More information about operator classes check:
   *
   * ### Useful links
   * https://www.postgresql.org/docs/current/sql-createindex.html
   *
   * https://www.postgresql.org/docs/current/indexes-opclass.html
   *
   * https://www.postgresql.org/docs/current/xindex.html
   *
   * ### Additional types
   * If you have the `pg_vector` extension installed in your database, you can use the
   * `vector_l2_ops`, `vector_ip_ops`, `vector_cosine_ops`, `vector_l1_ops`, `bit_hamming_ops`, `bit_jaccard_ops`, `halfvec_l2_ops`, `sparsevec_l2_ops` options, which are predefined types.
   *
   * **You can always specify any string you want in the operator class, in case Drizzle doesn't have it natively in its types**
   *
   * @param opClass
   * @returns
   */
  op(opClass) {
    this.indexConfig.opClass = opClass;
    return this;
  }
};
var IndexedColumn = class {
  static [entityKind] = "IndexedColumn";
  constructor(name, keyAsName, type, indexConfig) {
    this.name = name;
    this.keyAsName = keyAsName;
    this.type = type;
    this.indexConfig = indexConfig;
  }
  name;
  keyAsName;
  type;
  indexConfig;
};
var PgArrayBuilder = class extends PgColumnBuilder {
  static [entityKind] = "PgArrayBuilder";
  constructor(name, baseBuilder, size) {
    super(name, "array", "PgArray");
    this.config.baseBuilder = baseBuilder;
    this.config.size = size;
  }
  /** @internal */
  build(table2) {
    const baseColumn = this.config.baseBuilder.build(table2);
    return new PgArray(
      table2,
      this.config,
      baseColumn
    );
  }
};
var PgArray = class _PgArray extends PgColumn {
  constructor(table2, config, baseColumn, range) {
    super(table2, config);
    this.baseColumn = baseColumn;
    this.range = range;
    this.size = config.size;
  }
  size;
  static [entityKind] = "PgArray";
  getSQLType() {
    return `${this.baseColumn.getSQLType()}[${typeof this.size === "number" ? this.size : ""}]`;
  }
  mapFromDriverValue(value) {
    if (typeof value === "string") {
      value = parsePgArray(value);
    }
    return value.map((v) => this.baseColumn.mapFromDriverValue(v));
  }
  mapToDriverValue(value, isNestedArray = false) {
    const a = value.map(
      (v) => v === null ? null : is(this.baseColumn, _PgArray) ? this.baseColumn.mapToDriverValue(v, true) : this.baseColumn.mapToDriverValue(v)
    );
    if (isNestedArray) return a;
    return makePgArray(a);
  }
};

// node_modules/drizzle-orm/pg-core/columns/enum.js
var PgEnumObjectColumnBuilder = class extends PgColumnBuilder {
  static [entityKind] = "PgEnumObjectColumnBuilder";
  constructor(name, enumInstance) {
    super(name, "string", "PgEnumObjectColumn");
    this.config.enum = enumInstance;
  }
  /** @internal */
  build(table2) {
    return new PgEnumObjectColumn(
      table2,
      this.config
    );
  }
};
var PgEnumObjectColumn = class extends PgColumn {
  static [entityKind] = "PgEnumObjectColumn";
  enum;
  enumValues = this.config.enum.enumValues;
  constructor(table2, config) {
    super(table2, config);
    this.enum = config.enum;
  }
  getSQLType() {
    return this.enum.enumName;
  }
};
var isPgEnumSym = /* @__PURE__ */ Symbol.for("drizzle:isPgEnum");
function isPgEnum(obj) {
  return !!obj && typeof obj === "function" && isPgEnumSym in obj && obj[isPgEnumSym] === true;
}
var PgEnumColumnBuilder = class extends PgColumnBuilder {
  static [entityKind] = "PgEnumColumnBuilder";
  constructor(name, enumInstance) {
    super(name, "string", "PgEnumColumn");
    this.config.enum = enumInstance;
  }
  /** @internal */
  build(table2) {
    return new PgEnumColumn(
      table2,
      this.config
    );
  }
};
var PgEnumColumn = class extends PgColumn {
  static [entityKind] = "PgEnumColumn";
  enum = this.config.enum;
  enumValues = this.config.enum.enumValues;
  constructor(table2, config) {
    super(table2, config);
    this.enum = config.enum;
  }
  getSQLType() {
    return this.enum.enumName;
  }
};

// node_modules/drizzle-orm/subquery.js
var Subquery = class {
  static [entityKind] = "Subquery";
  constructor(sql2, fields, alias, isWith = false, usedTables = []) {
    this._ = {
      brand: "Subquery",
      sql: sql2,
      selectedFields: fields,
      alias,
      isWith,
      usedTables
    };
  }
  // getSQL(): SQL<unknown> {
  // 	return new SQL([this]);
  // }
};
var WithSubquery = class extends Subquery {
  static [entityKind] = "WithSubquery";
};

// node_modules/drizzle-orm/version.js
var version = "0.45.4";

// node_modules/drizzle-orm/tracing.js
var otel;
var rawTracer;
var tracer = {
  startActiveSpan(name, fn) {
    if (!otel) {
      return fn();
    }
    if (!rawTracer) {
      rawTracer = otel.trace.getTracer("drizzle-orm", version);
    }
    return iife(
      (otel2, rawTracer2) => rawTracer2.startActiveSpan(
        name,
        (span) => {
          try {
            return fn(span);
          } catch (e) {
            span.setStatus({
              code: otel2.SpanStatusCode.ERROR,
              message: e instanceof Error ? e.message : "Unknown error"
              // eslint-disable-line no-instanceof/no-instanceof
            });
            throw e;
          } finally {
            span.end();
          }
        }
      ),
      otel,
      rawTracer
    );
  }
};

// node_modules/drizzle-orm/view-common.js
var ViewBaseConfig = /* @__PURE__ */ Symbol.for("drizzle:ViewBaseConfig");

// node_modules/drizzle-orm/sql/sql.js
var FakePrimitiveParam = class {
  static [entityKind] = "FakePrimitiveParam";
};
function isSQLWrapper(value) {
  return value !== null && value !== void 0 && typeof value.getSQL === "function";
}
function mergeQueries(queries) {
  const result = { sql: "", params: [] };
  for (const query of queries) {
    result.sql += query.sql;
    result.params.push(...query.params);
    if (query.typings?.length) {
      if (!result.typings) {
        result.typings = [];
      }
      result.typings.push(...query.typings);
    }
  }
  return result;
}
var StringChunk = class {
  static [entityKind] = "StringChunk";
  value;
  constructor(value) {
    this.value = Array.isArray(value) ? value : [value];
  }
  getSQL() {
    return new SQL([this]);
  }
};
var SQL = class _SQL {
  constructor(queryChunks) {
    this.queryChunks = queryChunks;
    for (const chunk of queryChunks) {
      if (is(chunk, Table)) {
        const schemaName = chunk[Table.Symbol.Schema];
        this.usedTables.push(
          schemaName === void 0 ? chunk[Table.Symbol.Name] : schemaName + "." + chunk[Table.Symbol.Name]
        );
      }
    }
  }
  static [entityKind] = "SQL";
  /** @internal */
  decoder = noopDecoder;
  shouldInlineParams = false;
  /** @internal */
  usedTables = [];
  append(query) {
    this.queryChunks.push(...query.queryChunks);
    return this;
  }
  toQuery(config) {
    return tracer.startActiveSpan("drizzle.buildSQL", (span) => {
      const query = this.buildQueryFromSourceParams(this.queryChunks, config);
      span?.setAttributes({
        "drizzle.query.text": query.sql,
        "drizzle.query.params": JSON.stringify(query.params)
      });
      return query;
    });
  }
  buildQueryFromSourceParams(chunks, _config) {
    const config = Object.assign({}, _config, {
      inlineParams: _config.inlineParams || this.shouldInlineParams,
      paramStartIndex: _config.paramStartIndex || { value: 0 }
    });
    const {
      casing,
      escapeName,
      escapeParam,
      prepareTyping,
      inlineParams,
      paramStartIndex
    } = config;
    return mergeQueries(chunks.map((chunk) => {
      if (is(chunk, StringChunk)) {
        return { sql: chunk.value.join(""), params: [] };
      }
      if (is(chunk, Name)) {
        return { sql: escapeName(chunk.value), params: [] };
      }
      if (chunk === void 0) {
        return { sql: "", params: [] };
      }
      if (Array.isArray(chunk)) {
        const result = [new StringChunk("(")];
        for (const [i, p] of chunk.entries()) {
          result.push(p);
          if (i < chunk.length - 1) {
            result.push(new StringChunk(", "));
          }
        }
        result.push(new StringChunk(")"));
        return this.buildQueryFromSourceParams(result, config);
      }
      if (is(chunk, _SQL)) {
        return this.buildQueryFromSourceParams(chunk.queryChunks, {
          ...config,
          inlineParams: inlineParams || chunk.shouldInlineParams
        });
      }
      if (is(chunk, Table)) {
        const schemaName = chunk[Table.Symbol.Schema];
        const tableName = chunk[Table.Symbol.Name];
        return {
          sql: schemaName === void 0 || chunk[IsAlias] ? escapeName(tableName) : escapeName(schemaName) + "." + escapeName(tableName),
          params: []
        };
      }
      if (is(chunk, Column)) {
        const columnName = casing.getColumnCasing(chunk);
        if (_config.invokeSource === "indexes") {
          return { sql: escapeName(columnName), params: [] };
        }
        const schemaName = chunk.table[Table.Symbol.Schema];
        return {
          sql: chunk.table[IsAlias] || schemaName === void 0 ? escapeName(chunk.table[Table.Symbol.Name]) + "." + escapeName(columnName) : escapeName(schemaName) + "." + escapeName(chunk.table[Table.Symbol.Name]) + "." + escapeName(columnName),
          params: []
        };
      }
      if (is(chunk, View)) {
        const schemaName = chunk[ViewBaseConfig].schema;
        const viewName = chunk[ViewBaseConfig].name;
        return {
          sql: schemaName === void 0 || chunk[ViewBaseConfig].isAlias ? escapeName(viewName) : escapeName(schemaName) + "." + escapeName(viewName),
          params: []
        };
      }
      if (is(chunk, Param)) {
        if (is(chunk.value, Placeholder)) {
          return { sql: escapeParam(paramStartIndex.value++, chunk), params: [chunk], typings: ["none"] };
        }
        const mappedValue = chunk.value === null ? null : chunk.encoder.mapToDriverValue(chunk.value);
        if (is(mappedValue, _SQL)) {
          return this.buildQueryFromSourceParams([mappedValue], config);
        }
        if (inlineParams) {
          return { sql: this.mapInlineParam(mappedValue, config), params: [] };
        }
        let typings = ["none"];
        if (prepareTyping) {
          typings = [prepareTyping(chunk.encoder)];
        }
        return { sql: escapeParam(paramStartIndex.value++, mappedValue), params: [mappedValue], typings };
      }
      if (is(chunk, Placeholder)) {
        return { sql: escapeParam(paramStartIndex.value++, chunk), params: [chunk], typings: ["none"] };
      }
      if (is(chunk, _SQL.Aliased) && chunk.fieldAlias !== void 0) {
        return { sql: escapeName(chunk.fieldAlias), params: [] };
      }
      if (is(chunk, Subquery)) {
        if (chunk._.isWith) {
          return { sql: escapeName(chunk._.alias), params: [] };
        }
        return this.buildQueryFromSourceParams([
          new StringChunk("("),
          chunk._.sql,
          new StringChunk(") "),
          new Name(chunk._.alias)
        ], config);
      }
      if (isPgEnum(chunk)) {
        if (chunk.schema) {
          return { sql: escapeName(chunk.schema) + "." + escapeName(chunk.enumName), params: [] };
        }
        return { sql: escapeName(chunk.enumName), params: [] };
      }
      if (isSQLWrapper(chunk)) {
        if (chunk.shouldOmitSQLParens?.()) {
          return this.buildQueryFromSourceParams([chunk.getSQL()], config);
        }
        return this.buildQueryFromSourceParams([
          new StringChunk("("),
          chunk.getSQL(),
          new StringChunk(")")
        ], config);
      }
      if (inlineParams) {
        return { sql: this.mapInlineParam(chunk, config), params: [] };
      }
      return { sql: escapeParam(paramStartIndex.value++, chunk), params: [chunk], typings: ["none"] };
    }));
  }
  mapInlineParam(chunk, { escapeString }) {
    if (chunk === null) {
      return "null";
    }
    if (typeof chunk === "number" || typeof chunk === "boolean") {
      return chunk.toString();
    }
    if (typeof chunk === "string") {
      return escapeString(chunk);
    }
    if (typeof chunk === "object") {
      const mappedValueAsString = chunk.toString();
      if (mappedValueAsString === "[object Object]") {
        return escapeString(JSON.stringify(chunk));
      }
      return escapeString(mappedValueAsString);
    }
    throw new Error("Unexpected param value: " + chunk);
  }
  getSQL() {
    return this;
  }
  as(alias) {
    if (alias === void 0) {
      return this;
    }
    return new _SQL.Aliased(this, alias);
  }
  mapWith(decoder) {
    this.decoder = typeof decoder === "function" ? { mapFromDriverValue: decoder } : decoder;
    return this;
  }
  inlineParams() {
    this.shouldInlineParams = true;
    return this;
  }
  /**
   * This method is used to conditionally include a part of the query.
   *
   * @param condition - Condition to check
   * @returns itself if the condition is `true`, otherwise `undefined`
   */
  if(condition) {
    return condition ? this : void 0;
  }
};
var Name = class {
  constructor(value) {
    this.value = value;
  }
  static [entityKind] = "Name";
  brand;
  getSQL() {
    return new SQL([this]);
  }
};
function isDriverValueEncoder(value) {
  return typeof value === "object" && value !== null && "mapToDriverValue" in value && typeof value.mapToDriverValue === "function";
}
var noopDecoder = {
  mapFromDriverValue: (value) => value
};
var noopEncoder = {
  mapToDriverValue: (value) => value
};
var noopMapper = {
  ...noopDecoder,
  ...noopEncoder
};
var Param = class {
  /**
   * @param value - Parameter value
   * @param encoder - Encoder to convert the value to a driver parameter
   */
  constructor(value, encoder = noopEncoder) {
    this.value = value;
    this.encoder = encoder;
  }
  static [entityKind] = "Param";
  brand;
  getSQL() {
    return new SQL([this]);
  }
};
function sql(strings, ...params) {
  const queryChunks = [];
  if (params.length > 0 || strings.length > 0 && strings[0] !== "") {
    queryChunks.push(new StringChunk(strings[0]));
  }
  for (const [paramIndex, param2] of params.entries()) {
    queryChunks.push(param2, new StringChunk(strings[paramIndex + 1]));
  }
  return new SQL(queryChunks);
}
((sql2) => {
  function empty() {
    return new SQL([]);
  }
  sql2.empty = empty;
  function fromList(list) {
    return new SQL(list);
  }
  sql2.fromList = fromList;
  function raw(str) {
    return new SQL([new StringChunk(str)]);
  }
  sql2.raw = raw;
  function join(chunks, separator) {
    const result = [];
    for (const [i, chunk] of chunks.entries()) {
      if (i > 0 && separator !== void 0) {
        result.push(separator);
      }
      result.push(chunk);
    }
    return new SQL(result);
  }
  sql2.join = join;
  function identifier(value) {
    return new Name(value);
  }
  sql2.identifier = identifier;
  function placeholder2(name2) {
    return new Placeholder(name2);
  }
  sql2.placeholder = placeholder2;
  function param2(value, encoder) {
    return new Param(value, encoder);
  }
  sql2.param = param2;
})(sql || (sql = {}));
((SQL2) => {
  class Aliased {
    constructor(sql2, fieldAlias) {
      this.sql = sql2;
      this.fieldAlias = fieldAlias;
    }
    static [entityKind] = "SQL.Aliased";
    /** @internal */
    isSelectionField = false;
    getSQL() {
      return this.sql;
    }
    /** @internal */
    clone() {
      return new Aliased(this.sql, this.fieldAlias);
    }
  }
  SQL2.Aliased = Aliased;
})(SQL || (SQL = {}));
var Placeholder = class {
  constructor(name2) {
    this.name = name2;
  }
  static [entityKind] = "Placeholder";
  getSQL() {
    return new SQL([this]);
  }
};
function fillPlaceholders(params, values) {
  return params.map((p) => {
    if (is(p, Placeholder)) {
      if (!(p.name in values)) {
        throw new Error(`No value for placeholder "${p.name}" was provided`);
      }
      return values[p.name];
    }
    if (is(p, Param) && is(p.value, Placeholder)) {
      if (!(p.value.name in values)) {
        throw new Error(`No value for placeholder "${p.value.name}" was provided`);
      }
      return p.encoder.mapToDriverValue(values[p.value.name]);
    }
    return p;
  });
}
var IsDrizzleView = /* @__PURE__ */ Symbol.for("drizzle:IsDrizzleView");
var View = class {
  static [entityKind] = "View";
  /** @internal */
  [ViewBaseConfig];
  /** @internal */
  [IsDrizzleView] = true;
  constructor({ name: name2, schema, selectedFields, query }) {
    this[ViewBaseConfig] = {
      name: name2,
      originalName: name2,
      schema,
      selectedFields,
      query,
      isExisting: !query,
      isAlias: false
    };
  }
  getSQL() {
    return new SQL([this]);
  }
};
Column.prototype.getSQL = function() {
  return new SQL([this]);
};
Table.prototype.getSQL = function() {
  return new SQL([this]);
};
Subquery.prototype.getSQL = function() {
  return new SQL([this]);
};

// node_modules/drizzle-orm/utils.js
function mapResultRow(columns, row, joinsNotNullableMap) {
  const nullifyMap = {};
  const result = columns.reduce(
    (result2, { path, field }, columnIndex) => {
      let decoder;
      if (is(field, Column)) {
        decoder = field;
      } else if (is(field, SQL)) {
        decoder = field.decoder;
      } else if (is(field, Subquery)) {
        decoder = field._.sql.decoder;
      } else {
        decoder = field.sql.decoder;
      }
      let node = result2;
      for (const [pathChunkIndex, pathChunk] of path.entries()) {
        if (pathChunkIndex < path.length - 1) {
          if (!(pathChunk in node)) {
            node[pathChunk] = {};
          }
          node = node[pathChunk];
        } else {
          const rawValue = row[columnIndex];
          const value = node[pathChunk] = rawValue === null ? null : decoder.mapFromDriverValue(rawValue);
          if (joinsNotNullableMap && is(field, Column) && path.length === 2) {
            const objectName = path[0];
            if (!(objectName in nullifyMap)) {
              nullifyMap[objectName] = value === null ? getTableName(field.table) : false;
            } else if (typeof nullifyMap[objectName] === "string" && nullifyMap[objectName] !== getTableName(field.table)) {
              nullifyMap[objectName] = false;
            }
          }
        }
      }
      return result2;
    },
    {}
  );
  if (joinsNotNullableMap && Object.keys(nullifyMap).length > 0) {
    for (const [objectName, tableName] of Object.entries(nullifyMap)) {
      if (typeof tableName === "string" && !joinsNotNullableMap[tableName]) {
        result[objectName] = null;
      }
    }
  }
  return result;
}
function orderSelectedFields(fields, pathPrefix) {
  return Object.entries(fields).reduce((result, [name, field]) => {
    if (typeof name !== "string") {
      return result;
    }
    const newPath = pathPrefix ? [...pathPrefix, name] : [name];
    if (is(field, Column) || is(field, SQL) || is(field, SQL.Aliased) || is(field, Subquery)) {
      result.push({ path: newPath, field });
    } else if (is(field, Table)) {
      result.push(...orderSelectedFields(field[Table.Symbol.Columns], newPath));
    } else {
      result.push(...orderSelectedFields(field, newPath));
    }
    return result;
  }, []);
}
function haveSameKeys(left, right) {
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  if (leftKeys.length !== rightKeys.length) {
    return false;
  }
  for (const [index, key] of leftKeys.entries()) {
    if (key !== rightKeys[index]) {
      return false;
    }
  }
  return true;
}
function mapUpdateSet(table2, values) {
  const entries = Object.entries(values).filter(([, value]) => value !== void 0).map(([key, value]) => {
    if (is(value, SQL) || is(value, Column)) {
      return [key, value];
    } else {
      return [key, new Param(value, table2[Table.Symbol.Columns][key])];
    }
  });
  if (entries.length === 0) {
    throw new Error("No values to set");
  }
  return Object.fromEntries(entries);
}
function applyMixins(baseClass, extendedClasses) {
  for (const extendedClass of extendedClasses) {
    for (const name of Object.getOwnPropertyNames(extendedClass.prototype)) {
      if (name === "constructor") continue;
      Object.defineProperty(
        baseClass.prototype,
        name,
        Object.getOwnPropertyDescriptor(extendedClass.prototype, name) || /* @__PURE__ */ Object.create(null)
      );
    }
  }
}
function getTableColumns(table2) {
  return table2[Table.Symbol.Columns];
}
function getTableLikeName(table2) {
  return is(table2, Subquery) ? table2._.alias : is(table2, View) ? table2[ViewBaseConfig].name : is(table2, SQL) ? void 0 : table2[Table.Symbol.IsAlias] ? table2[Table.Symbol.Name] : table2[Table.Symbol.BaseName];
}
var textDecoder = typeof TextDecoder === "undefined" ? null : new TextDecoder();

// node_modules/drizzle-orm/pg-core/table.js
var InlineForeignKeys = /* @__PURE__ */ Symbol.for("drizzle:PgInlineForeignKeys");
var EnableRLS = /* @__PURE__ */ Symbol.for("drizzle:EnableRLS");
var PgTable = class extends Table {
  static [entityKind] = "PgTable";
  /** @internal */
  static Symbol = Object.assign({}, Table.Symbol, {
    InlineForeignKeys,
    EnableRLS
  });
  /**@internal */
  [InlineForeignKeys] = [];
  /** @internal */
  [EnableRLS] = false;
  /** @internal */
  [Table.Symbol.ExtraConfigBuilder] = void 0;
  /** @internal */
  [Table.Symbol.ExtraConfigColumns] = {};
};

// node_modules/drizzle-orm/pg-core/primary-keys.js
var PrimaryKeyBuilder = class {
  static [entityKind] = "PgPrimaryKeyBuilder";
  /** @internal */
  columns;
  /** @internal */
  name;
  constructor(columns, name) {
    this.columns = columns;
    this.name = name;
  }
  /** @internal */
  build(table2) {
    return new PrimaryKey(table2, this.columns, this.name);
  }
};
var PrimaryKey = class {
  constructor(table2, columns, name) {
    this.table = table2;
    this.columns = columns;
    this.name = name;
  }
  static [entityKind] = "PgPrimaryKey";
  columns;
  name;
  getName() {
    return this.name ?? `${this.table[PgTable.Symbol.Name]}_${this.columns.map((column) => column.name).join("_")}_pk`;
  }
};

// node_modules/drizzle-orm/sql/expressions/conditions.js
function bindIfParam(value, column) {
  if (isDriverValueEncoder(column) && !isSQLWrapper(value) && !is(value, Param) && !is(value, Placeholder) && !is(value, Column) && !is(value, Table) && !is(value, View)) {
    return new Param(value, column);
  }
  return value;
}
var eq = (left, right) => {
  return sql`${left} = ${bindIfParam(right, left)}`;
};
var ne = (left, right) => {
  return sql`${left} <> ${bindIfParam(right, left)}`;
};
function and(...unfilteredConditions) {
  const conditions = unfilteredConditions.filter(
    (c) => c !== void 0
  );
  if (conditions.length === 0) {
    return void 0;
  }
  if (conditions.length === 1) {
    return new SQL(conditions);
  }
  return new SQL([
    new StringChunk("("),
    sql.join(conditions, new StringChunk(" and ")),
    new StringChunk(")")
  ]);
}
function or(...unfilteredConditions) {
  const conditions = unfilteredConditions.filter(
    (c) => c !== void 0
  );
  if (conditions.length === 0) {
    return void 0;
  }
  if (conditions.length === 1) {
    return new SQL(conditions);
  }
  return new SQL([
    new StringChunk("("),
    sql.join(conditions, new StringChunk(" or ")),
    new StringChunk(")")
  ]);
}
function not(condition) {
  return sql`not ${condition}`;
}
var gt = (left, right) => {
  return sql`${left} > ${bindIfParam(right, left)}`;
};
var gte = (left, right) => {
  return sql`${left} >= ${bindIfParam(right, left)}`;
};
var lt = (left, right) => {
  return sql`${left} < ${bindIfParam(right, left)}`;
};
var lte = (left, right) => {
  return sql`${left} <= ${bindIfParam(right, left)}`;
};
function inArray(column, values) {
  if (Array.isArray(values)) {
    if (values.length === 0) {
      return sql`false`;
    }
    return sql`${column} in ${values.map((v) => bindIfParam(v, column))}`;
  }
  return sql`${column} in ${bindIfParam(values, column)}`;
}
function notInArray(column, values) {
  if (Array.isArray(values)) {
    if (values.length === 0) {
      return sql`true`;
    }
    return sql`${column} not in ${values.map((v) => bindIfParam(v, column))}`;
  }
  return sql`${column} not in ${bindIfParam(values, column)}`;
}
function isNull(value) {
  return sql`${value} is null`;
}
function isNotNull(value) {
  return sql`${value} is not null`;
}
function exists(subquery) {
  return sql`exists ${subquery}`;
}
function notExists(subquery) {
  return sql`not exists ${subquery}`;
}
function between(column, min, max) {
  return sql`${column} between ${bindIfParam(min, column)} and ${bindIfParam(
    max,
    column
  )}`;
}
function notBetween(column, min, max) {
  return sql`${column} not between ${bindIfParam(
    min,
    column
  )} and ${bindIfParam(max, column)}`;
}
function like(column, value) {
  return sql`${column} like ${value}`;
}
function notLike(column, value) {
  return sql`${column} not like ${value}`;
}
function ilike(column, value) {
  return sql`${column} ilike ${value}`;
}
function notIlike(column, value) {
  return sql`${column} not ilike ${value}`;
}

// node_modules/drizzle-orm/sql/expressions/select.js
function asc(column) {
  return sql`${column} asc`;
}
function desc(column) {
  return sql`${column} desc`;
}

// node_modules/drizzle-orm/relations.js
var Relation = class {
  constructor(sourceTable, referencedTable, relationName) {
    this.sourceTable = sourceTable;
    this.referencedTable = referencedTable;
    this.relationName = relationName;
    this.referencedTableName = referencedTable[Table.Symbol.Name];
  }
  static [entityKind] = "Relation";
  referencedTableName;
  fieldName;
};
var Relations = class {
  constructor(table2, config) {
    this.table = table2;
    this.config = config;
  }
  static [entityKind] = "Relations";
};
var One = class _One extends Relation {
  constructor(sourceTable, referencedTable, config, isNullable) {
    super(sourceTable, referencedTable, config?.relationName);
    this.config = config;
    this.isNullable = isNullable;
  }
  static [entityKind] = "One";
  withFieldName(fieldName) {
    const relation = new _One(
      this.sourceTable,
      this.referencedTable,
      this.config,
      this.isNullable
    );
    relation.fieldName = fieldName;
    return relation;
  }
};
var Many = class _Many extends Relation {
  constructor(sourceTable, referencedTable, config) {
    super(sourceTable, referencedTable, config?.relationName);
    this.config = config;
  }
  static [entityKind] = "Many";
  withFieldName(fieldName) {
    const relation = new _Many(
      this.sourceTable,
      this.referencedTable,
      this.config
    );
    relation.fieldName = fieldName;
    return relation;
  }
};
function getOperators() {
  return {
    and,
    between,
    eq,
    exists,
    gt,
    gte,
    ilike,
    inArray,
    isNull,
    isNotNull,
    like,
    lt,
    lte,
    ne,
    not,
    notBetween,
    notExists,
    notLike,
    notIlike,
    notInArray,
    or,
    sql
  };
}
function getOrderByOperators() {
  return {
    sql,
    asc,
    desc
  };
}
function extractTablesRelationalConfig(schema, configHelpers) {
  if (Object.keys(schema).length === 1 && "default" in schema && !is(schema["default"], Table)) {
    schema = schema["default"];
  }
  const tableNamesMap = {};
  const relationsBuffer = {};
  const tablesConfig = {};
  for (const [key, value] of Object.entries(schema)) {
    if (is(value, Table)) {
      const dbName = getTableUniqueName(value);
      const bufferedRelations = relationsBuffer[dbName];
      tableNamesMap[dbName] = key;
      tablesConfig[key] = {
        tsName: key,
        dbName: value[Table.Symbol.Name],
        schema: value[Table.Symbol.Schema],
        columns: value[Table.Symbol.Columns],
        relations: bufferedRelations?.relations ?? {},
        primaryKey: bufferedRelations?.primaryKey ?? []
      };
      for (const column of Object.values(
        value[Table.Symbol.Columns]
      )) {
        if (column.primary) {
          tablesConfig[key].primaryKey.push(column);
        }
      }
      const extraConfig = value[Table.Symbol.ExtraConfigBuilder]?.(value[Table.Symbol.ExtraConfigColumns]);
      if (extraConfig) {
        for (const configEntry of Object.values(extraConfig)) {
          if (is(configEntry, PrimaryKeyBuilder)) {
            tablesConfig[key].primaryKey.push(...configEntry.columns);
          }
        }
      }
    } else if (is(value, Relations)) {
      const dbName = getTableUniqueName(value.table);
      const tableName = tableNamesMap[dbName];
      const relations2 = value.config(
        configHelpers(value.table)
      );
      let primaryKey;
      for (const [relationName, relation] of Object.entries(relations2)) {
        if (tableName) {
          const tableConfig = tablesConfig[tableName];
          tableConfig.relations[relationName] = relation;
          if (primaryKey) {
            tableConfig.primaryKey.push(...primaryKey);
          }
        } else {
          if (!(dbName in relationsBuffer)) {
            relationsBuffer[dbName] = {
              relations: {},
              primaryKey
            };
          }
          relationsBuffer[dbName].relations[relationName] = relation;
        }
      }
    }
  }
  return { tables: tablesConfig, tableNamesMap };
}
function createOne(sourceTable) {
  return function one(table2, config) {
    return new One(
      sourceTable,
      table2,
      config,
      config?.fields.reduce((res, f) => res && f.notNull, true) ?? false
    );
  };
}
function createMany(sourceTable) {
  return function many(referencedTable, config) {
    return new Many(sourceTable, referencedTable, config);
  };
}
function normalizeRelation(schema, tableNamesMap, relation) {
  if (is(relation, One) && relation.config) {
    return {
      fields: relation.config.fields,
      references: relation.config.references
    };
  }
  const referencedTableTsName = tableNamesMap[getTableUniqueName(relation.referencedTable)];
  if (!referencedTableTsName) {
    throw new Error(
      `Table "${relation.referencedTable[Table.Symbol.Name]}" not found in schema`
    );
  }
  const referencedTableConfig = schema[referencedTableTsName];
  if (!referencedTableConfig) {
    throw new Error(`Table "${referencedTableTsName}" not found in schema`);
  }
  const sourceTable = relation.sourceTable;
  const sourceTableTsName = tableNamesMap[getTableUniqueName(sourceTable)];
  if (!sourceTableTsName) {
    throw new Error(
      `Table "${sourceTable[Table.Symbol.Name]}" not found in schema`
    );
  }
  const reverseRelations = [];
  for (const referencedTableRelation of Object.values(
    referencedTableConfig.relations
  )) {
    if (relation.relationName && relation !== referencedTableRelation && referencedTableRelation.relationName === relation.relationName || !relation.relationName && referencedTableRelation.referencedTable === relation.sourceTable) {
      reverseRelations.push(referencedTableRelation);
    }
  }
  if (reverseRelations.length > 1) {
    throw relation.relationName ? new Error(
      `There are multiple relations with name "${relation.relationName}" in table "${referencedTableTsName}"`
    ) : new Error(
      `There are multiple relations between "${referencedTableTsName}" and "${relation.sourceTable[Table.Symbol.Name]}". Please specify relation name`
    );
  }
  if (reverseRelations[0] && is(reverseRelations[0], One) && reverseRelations[0].config) {
    return {
      fields: reverseRelations[0].config.references,
      references: reverseRelations[0].config.fields
    };
  }
  throw new Error(
    `There is not enough information to infer relation "${sourceTableTsName}.${relation.fieldName}"`
  );
}
function createTableRelationsHelpers(sourceTable) {
  return {
    one: createOne(sourceTable),
    many: createMany(sourceTable)
  };
}
function mapRelationalRow(tablesConfig, tableConfig, row, buildQueryResultSelection, mapColumnValue = (value) => value) {
  const result = {};
  for (const [
    selectionItemIndex,
    selectionItem
  ] of buildQueryResultSelection.entries()) {
    if (selectionItem.isJson) {
      const relation = tableConfig.relations[selectionItem.tsKey];
      const rawSubRows = row[selectionItemIndex];
      const subRows = typeof rawSubRows === "string" ? JSON.parse(rawSubRows) : rawSubRows;
      result[selectionItem.tsKey] = is(relation, One) ? subRows && mapRelationalRow(
        tablesConfig,
        tablesConfig[selectionItem.relationTableTsKey],
        subRows,
        selectionItem.selection,
        mapColumnValue
      ) : subRows.map(
        (subRow) => mapRelationalRow(
          tablesConfig,
          tablesConfig[selectionItem.relationTableTsKey],
          subRow,
          selectionItem.selection,
          mapColumnValue
        )
      );
    } else {
      const value = mapColumnValue(row[selectionItemIndex]);
      const field = selectionItem.field;
      let decoder;
      if (is(field, Column)) {
        decoder = field;
      } else if (is(field, SQL)) {
        decoder = field.decoder;
      } else {
        decoder = field.sql.decoder;
      }
      result[selectionItem.tsKey] = value === null ? null : decoder.mapFromDriverValue(value);
    }
  }
  return result;
}

// node_modules/drizzle-orm/alias.js
var ColumnAliasProxyHandler = class {
  constructor(table2) {
    this.table = table2;
  }
  static [entityKind] = "ColumnAliasProxyHandler";
  get(columnObj, prop) {
    if (prop === "table") {
      return this.table;
    }
    return columnObj[prop];
  }
};
var TableAliasProxyHandler = class {
  constructor(alias, replaceOriginalName) {
    this.alias = alias;
    this.replaceOriginalName = replaceOriginalName;
  }
  static [entityKind] = "TableAliasProxyHandler";
  get(target, prop) {
    if (prop === Table.Symbol.IsAlias) {
      return true;
    }
    if (prop === Table.Symbol.Name) {
      return this.alias;
    }
    if (this.replaceOriginalName && prop === Table.Symbol.OriginalName) {
      return this.alias;
    }
    if (prop === ViewBaseConfig) {
      return {
        ...target[ViewBaseConfig],
        name: this.alias,
        isAlias: true
      };
    }
    if (prop === Table.Symbol.Columns) {
      const columns = target[Table.Symbol.Columns];
      if (!columns) {
        return columns;
      }
      const proxiedColumns = {};
      Object.keys(columns).map((key) => {
        proxiedColumns[key] = new Proxy(
          columns[key],
          new ColumnAliasProxyHandler(new Proxy(target, this))
        );
      });
      return proxiedColumns;
    }
    const value = target[prop];
    if (is(value, Column)) {
      return new Proxy(value, new ColumnAliasProxyHandler(new Proxy(target, this)));
    }
    return value;
  }
};
var RelationTableAliasProxyHandler = class {
  constructor(alias) {
    this.alias = alias;
  }
  static [entityKind] = "RelationTableAliasProxyHandler";
  get(target, prop) {
    if (prop === "sourceTable") {
      return aliasedTable(target.sourceTable, this.alias);
    }
    return target[prop];
  }
};
function aliasedTable(table2, tableAlias) {
  return new Proxy(table2, new TableAliasProxyHandler(tableAlias, false));
}
function aliasedTableColumn(column, tableAlias) {
  return new Proxy(
    column,
    new ColumnAliasProxyHandler(new Proxy(column.table, new TableAliasProxyHandler(tableAlias, false)))
  );
}
function mapColumnsInAliasedSQLToAlias(query, alias) {
  return new SQL.Aliased(mapColumnsInSQLToAlias(query.sql, alias), query.fieldAlias);
}
function mapColumnsInSQLToAlias(query, alias) {
  return sql.join(query.queryChunks.map((c) => {
    if (is(c, Column)) {
      return aliasedTableColumn(c, alias);
    }
    if (is(c, SQL)) {
      return mapColumnsInSQLToAlias(c, alias);
    }
    if (is(c, SQL.Aliased)) {
      return mapColumnsInAliasedSQLToAlias(c, alias);
    }
    return c;
  }));
}

// node_modules/drizzle-orm/selection-proxy.js
var SelectionProxyHandler = class _SelectionProxyHandler {
  static [entityKind] = "SelectionProxyHandler";
  config;
  constructor(config) {
    this.config = { ...config };
  }
  get(subquery, prop) {
    if (prop === "_") {
      return {
        ...subquery["_"],
        selectedFields: new Proxy(
          subquery._.selectedFields,
          this
        )
      };
    }
    if (prop === ViewBaseConfig) {
      return {
        ...subquery[ViewBaseConfig],
        selectedFields: new Proxy(
          subquery[ViewBaseConfig].selectedFields,
          this
        )
      };
    }
    if (typeof prop === "symbol") {
      return subquery[prop];
    }
    const columns = is(subquery, Subquery) ? subquery._.selectedFields : is(subquery, View) ? subquery[ViewBaseConfig].selectedFields : subquery;
    const value = columns[prop];
    if (is(value, SQL.Aliased)) {
      if (this.config.sqlAliasedBehavior === "sql" && !value.isSelectionField) {
        return value.sql;
      }
      const newValue = value.clone();
      newValue.isSelectionField = true;
      return newValue;
    }
    if (is(value, SQL)) {
      if (this.config.sqlBehavior === "sql") {
        return value;
      }
      throw new Error(
        `You tried to reference "${prop}" field from a subquery, which is a raw SQL field, but it doesn't have an alias declared. Please add an alias to the field using ".as('alias')" method.`
      );
    }
    if (is(value, Column)) {
      if (this.config.alias) {
        return new Proxy(
          value,
          new ColumnAliasProxyHandler(
            new Proxy(
              value.table,
              new TableAliasProxyHandler(this.config.alias, this.config.replaceOriginalName ?? false)
            )
          )
        );
      }
      return value;
    }
    if (typeof value !== "object" || value === null) {
      return value;
    }
    return new Proxy(value, new _SelectionProxyHandler(this.config));
  }
};

// node_modules/drizzle-orm/query-promise.js
var QueryPromise = class {
  static [entityKind] = "QueryPromise";
  [Symbol.toStringTag] = "QueryPromise";
  catch(onRejected) {
    return this.then(void 0, onRejected);
  }
  finally(onFinally) {
    return this.then(
      (value) => {
        onFinally?.();
        return value;
      },
      (reason) => {
        onFinally?.();
        throw reason;
      }
    );
  }
  then(onFulfilled, onRejected) {
    return this.execute().then(onFulfilled, onRejected);
  }
};

// node_modules/drizzle-orm/sqlite-core/foreign-keys.js
var ForeignKeyBuilder2 = class {
  static [entityKind] = "SQLiteForeignKeyBuilder";
  /** @internal */
  reference;
  /** @internal */
  _onUpdate;
  /** @internal */
  _onDelete;
  constructor(config, actions) {
    this.reference = () => {
      const { name, columns, foreignColumns } = config();
      return { name, columns, foreignTable: foreignColumns[0].table, foreignColumns };
    };
    if (actions) {
      this._onUpdate = actions.onUpdate;
      this._onDelete = actions.onDelete;
    }
  }
  onUpdate(action) {
    this._onUpdate = action;
    return this;
  }
  onDelete(action) {
    this._onDelete = action;
    return this;
  }
  /** @internal */
  build(table2) {
    return new ForeignKey2(table2, this);
  }
};
var ForeignKey2 = class {
  constructor(table2, builder) {
    this.table = table2;
    this.reference = builder.reference;
    this.onUpdate = builder._onUpdate;
    this.onDelete = builder._onDelete;
  }
  static [entityKind] = "SQLiteForeignKey";
  reference;
  onUpdate;
  onDelete;
  getName() {
    const { name, columns, foreignColumns } = this.reference();
    const columnNames = columns.map((column) => column.name);
    const foreignColumnNames = foreignColumns.map((column) => column.name);
    const chunks = [
      this.table[TableName],
      ...columnNames,
      foreignColumns[0].table[TableName],
      ...foreignColumnNames
    ];
    return name ?? `${chunks.join("_")}_fk`;
  }
};

// node_modules/drizzle-orm/sqlite-core/unique-constraint.js
function uniqueKeyName2(table2, columns) {
  return `${table2[TableName]}_${columns.join("_")}_unique`;
}
var UniqueConstraintBuilder2 = class {
  constructor(columns, name) {
    this.name = name;
    this.columns = columns;
  }
  static [entityKind] = "SQLiteUniqueConstraintBuilder";
  /** @internal */
  columns;
  /** @internal */
  build(table2) {
    return new UniqueConstraint2(table2, this.columns, this.name);
  }
};
var UniqueOnConstraintBuilder2 = class {
  static [entityKind] = "SQLiteUniqueOnConstraintBuilder";
  /** @internal */
  name;
  constructor(name) {
    this.name = name;
  }
  on(...columns) {
    return new UniqueConstraintBuilder2(columns, this.name);
  }
};
var UniqueConstraint2 = class {
  constructor(table2, columns, name) {
    this.table = table2;
    this.columns = columns;
    this.name = name ?? uniqueKeyName2(this.table, this.columns.map((column) => column.name));
  }
  static [entityKind] = "SQLiteUniqueConstraint";
  columns;
  name;
  getName() {
    return this.name;
  }
};

// node_modules/drizzle-orm/sqlite-core/columns/common.js
var SQLiteColumnBuilder = class extends ColumnBuilder {
  static [entityKind] = "SQLiteColumnBuilder";
  foreignKeyConfigs = [];
  references(ref, actions = {}) {
    this.foreignKeyConfigs.push({ ref, actions });
    return this;
  }
  unique(name) {
    this.config.isUnique = true;
    this.config.uniqueName = name;
    return this;
  }
  generatedAlwaysAs(as, config) {
    this.config.generated = {
      as,
      type: "always",
      mode: config?.mode ?? "virtual"
    };
    return this;
  }
  /** @internal */
  buildForeignKeys(column, table2) {
    return this.foreignKeyConfigs.map(({ ref, actions }) => {
      return ((ref2, actions2) => {
        const builder = new ForeignKeyBuilder2(() => {
          const foreignColumn = ref2();
          return { columns: [column], foreignColumns: [foreignColumn] };
        });
        if (actions2.onUpdate) {
          builder.onUpdate(actions2.onUpdate);
        }
        if (actions2.onDelete) {
          builder.onDelete(actions2.onDelete);
        }
        return builder.build(table2);
      })(ref, actions);
    });
  }
};
var SQLiteColumn = class extends Column {
  constructor(table2, config) {
    if (!config.uniqueName) {
      config.uniqueName = uniqueKeyName2(table2, [config.name]);
    }
    super(table2, config);
    this.table = table2;
  }
  static [entityKind] = "SQLiteColumn";
};

// node_modules/drizzle-orm/sqlite-core/table.js
var InlineForeignKeys2 = /* @__PURE__ */ Symbol.for("drizzle:SQLiteInlineForeignKeys");
var SQLiteTable = class extends Table {
  static [entityKind] = "SQLiteTable";
  /** @internal */
  static Symbol = Object.assign({}, Table.Symbol, {
    InlineForeignKeys: InlineForeignKeys2
  });
  /** @internal */
  [Table.Symbol.Columns];
  /** @internal */
  [InlineForeignKeys2] = [];
  /** @internal */
  [Table.Symbol.ExtraConfigBuilder] = void 0;
};

// node_modules/drizzle-orm/sqlite-core/utils.js
function extractUsedTable(table2) {
  if (is(table2, SQLiteTable)) {
    return [`${table2[Table.Symbol.BaseName]}`];
  }
  if (is(table2, Subquery)) {
    return table2._.usedTables ?? [];
  }
  if (is(table2, SQL)) {
    return table2.usedTables ?? [];
  }
  return [];
}

// node_modules/drizzle-orm/sqlite-core/query-builders/delete.js
var SQLiteDeleteBase = class extends QueryPromise {
  constructor(table2, session, dialect, withList) {
    super();
    this.table = table2;
    this.session = session;
    this.dialect = dialect;
    this.config = { table: table2, withList };
  }
  static [entityKind] = "SQLiteDelete";
  /** @internal */
  config;
  /**
   * Adds a `where` clause to the query.
   *
   * Calling this method will delete only those rows that fulfill a specified condition.
   *
   * See docs: {@link https://orm.drizzle.team/docs/delete}
   *
   * @param where the `where` clause.
   *
   * @example
   * You can use conditional operators and `sql function` to filter the rows to be deleted.
   *
   * ```ts
   * // Delete all cars with green color
   * db.delete(cars).where(eq(cars.color, 'green'));
   * // or
   * db.delete(cars).where(sql`${cars.color} = 'green'`)
   * ```
   *
   * You can logically combine conditional operators with `and()` and `or()` operators:
   *
   * ```ts
   * // Delete all BMW cars with a green color
   * db.delete(cars).where(and(eq(cars.color, 'green'), eq(cars.brand, 'BMW')));
   *
   * // Delete all cars with the green or blue color
   * db.delete(cars).where(or(eq(cars.color, 'green'), eq(cars.color, 'blue')));
   * ```
   */
  where(where) {
    this.config.where = where;
    return this;
  }
  orderBy(...columns) {
    if (typeof columns[0] === "function") {
      const orderBy = columns[0](
        new Proxy(
          this.config.table[Table.Symbol.Columns],
          new SelectionProxyHandler({ sqlAliasedBehavior: "alias", sqlBehavior: "sql" })
        )
      );
      const orderByArray = Array.isArray(orderBy) ? orderBy : [orderBy];
      this.config.orderBy = orderByArray;
    } else {
      const orderByArray = columns;
      this.config.orderBy = orderByArray;
    }
    return this;
  }
  limit(limit) {
    this.config.limit = limit;
    return this;
  }
  returning(fields = this.table[SQLiteTable.Symbol.Columns]) {
    this.config.returning = orderSelectedFields(fields);
    return this;
  }
  /** @internal */
  getSQL() {
    return this.dialect.buildDeleteQuery(this.config);
  }
  toSQL() {
    const { typings: _typings, ...rest } = this.dialect.sqlToQuery(this.getSQL());
    return rest;
  }
  /** @internal */
  _prepare(isOneTimeQuery = true) {
    return this.session[isOneTimeQuery ? "prepareOneTimeQuery" : "prepareQuery"](
      this.dialect.sqlToQuery(this.getSQL()),
      this.config.returning,
      this.config.returning ? "all" : "run",
      true,
      void 0,
      {
        type: "delete",
        tables: extractUsedTable(this.config.table)
      }
    );
  }
  prepare() {
    return this._prepare(false);
  }
  run = (placeholderValues) => {
    return this._prepare().run(placeholderValues);
  };
  all = (placeholderValues) => {
    return this._prepare().all(placeholderValues);
  };
  get = (placeholderValues) => {
    return this._prepare().get(placeholderValues);
  };
  values = (placeholderValues) => {
    return this._prepare().values(placeholderValues);
  };
  async execute(placeholderValues) {
    return this._prepare().execute(placeholderValues);
  }
  $dynamic() {
    return this;
  }
};

// node_modules/drizzle-orm/casing.js
function toSnakeCase(input) {
  const words = input.replace(/['\u2019]/g, "").match(/[\da-z]+|[A-Z]+(?![a-z])|[A-Z][\da-z]+/g) ?? [];
  return words.map((word) => word.toLowerCase()).join("_");
}
function toCamelCase(input) {
  const words = input.replace(/['\u2019]/g, "").match(/[\da-z]+|[A-Z]+(?![a-z])|[A-Z][\da-z]+/g) ?? [];
  return words.reduce((acc, word, i) => {
    const formattedWord = i === 0 ? word.toLowerCase() : `${word[0].toUpperCase()}${word.slice(1)}`;
    return acc + formattedWord;
  }, "");
}
function noopCase(input) {
  return input;
}
var CasingCache = class {
  static [entityKind] = "CasingCache";
  /** @internal */
  cache = {};
  cachedTables = {};
  convert;
  constructor(casing) {
    this.convert = casing === "snake_case" ? toSnakeCase : casing === "camelCase" ? toCamelCase : noopCase;
  }
  getColumnCasing(column) {
    if (!column.keyAsName) return column.name;
    const schema = column.table[Table.Symbol.Schema] ?? "public";
    const tableName = column.table[Table.Symbol.OriginalName];
    const key = `${schema}.${tableName}.${column.name}`;
    if (!this.cache[key]) {
      this.cacheTable(column.table);
    }
    return this.cache[key];
  }
  cacheTable(table2) {
    const schema = table2[Table.Symbol.Schema] ?? "public";
    const tableName = table2[Table.Symbol.OriginalName];
    const tableKey = `${schema}.${tableName}`;
    if (!this.cachedTables[tableKey]) {
      for (const column of Object.values(table2[Table.Symbol.Columns])) {
        const columnKey = `${tableKey}.${column.name}`;
        this.cache[columnKey] = this.convert(column.name);
      }
      this.cachedTables[tableKey] = true;
    }
  }
  clearCache() {
    this.cache = {};
    this.cachedTables = {};
  }
};

// node_modules/drizzle-orm/errors.js
var DrizzleError = class extends Error {
  static [entityKind] = "DrizzleError";
  constructor({ message, cause }) {
    super(message);
    this.name = "DrizzleError";
    this.cause = cause;
  }
};
var DrizzleQueryError = class _DrizzleQueryError extends Error {
  constructor(query, params, cause) {
    super(`Failed query: ${query}
params: ${params}`);
    this.query = query;
    this.params = params;
    this.cause = cause;
    Error.captureStackTrace(this, _DrizzleQueryError);
    if (cause) this.cause = cause;
  }
};
var TransactionRollbackError = class extends DrizzleError {
  static [entityKind] = "TransactionRollbackError";
  constructor() {
    super({ message: "Rollback" });
  }
};

// node_modules/drizzle-orm/sqlite-core/view-base.js
var SQLiteViewBase = class extends View {
  static [entityKind] = "SQLiteViewBase";
};

// node_modules/drizzle-orm/sqlite-core/dialect.js
var SQLiteDialect = class {
  static [entityKind] = "SQLiteDialect";
  /** @internal */
  casing;
  constructor(config) {
    this.casing = new CasingCache(config?.casing);
  }
  escapeName(name) {
    return `"${name.replace(/"/g, '""')}"`;
  }
  escapeParam(_num) {
    return "?";
  }
  escapeString(str) {
    return `'${str.replace(/'/g, "''")}'`;
  }
  buildWithCTE(queries) {
    if (!queries?.length) return void 0;
    const withSqlChunks = [sql`with `];
    for (const [i, w] of queries.entries()) {
      withSqlChunks.push(sql`${sql.identifier(w._.alias)} as (${w._.sql})`);
      if (i < queries.length - 1) {
        withSqlChunks.push(sql`, `);
      }
    }
    withSqlChunks.push(sql` `);
    return sql.join(withSqlChunks);
  }
  buildDeleteQuery({
    table: table2,
    where,
    returning,
    withList,
    limit,
    orderBy
  }) {
    const withSql = this.buildWithCTE(withList);
    const returningSql = returning ? sql` returning ${this.buildSelection(returning, { isSingleTable: true })}` : void 0;
    const whereSql = where ? sql` where ${where}` : void 0;
    const orderBySql = this.buildOrderBy(orderBy);
    const limitSql = this.buildLimit(limit);
    return sql`${withSql}delete from ${table2}${whereSql}${returningSql}${orderBySql}${limitSql}`;
  }
  buildUpdateSet(table2, set) {
    const tableColumns = table2[Table.Symbol.Columns];
    const columnNames = Object.keys(tableColumns).filter(
      (colName) => set[colName] !== void 0 || tableColumns[colName]?.onUpdateFn !== void 0
    );
    const setSize = columnNames.length;
    return sql.join(
      columnNames.flatMap((colName, i) => {
        const col = tableColumns[colName];
        const onUpdateFnResult = col.onUpdateFn?.();
        const value = set[colName] ?? (is(onUpdateFnResult, SQL) ? onUpdateFnResult : sql.param(onUpdateFnResult, col));
        const res = sql`${sql.identifier(this.casing.getColumnCasing(col))} = ${value}`;
        if (i < setSize - 1) {
          return [res, sql.raw(", ")];
        }
        return [res];
      })
    );
  }
  buildUpdateQuery({
    table: table2,
    set,
    where,
    returning,
    withList,
    joins,
    from,
    limit,
    orderBy
  }) {
    const withSql = this.buildWithCTE(withList);
    const setSql = this.buildUpdateSet(table2, set);
    const fromSql = from && sql.join([sql.raw(" from "), this.buildFromTable(from)]);
    const joinsSql = this.buildJoins(joins);
    const returningSql = returning ? sql` returning ${this.buildSelection(returning, { isSingleTable: true })}` : void 0;
    const whereSql = where ? sql` where ${where}` : void 0;
    const orderBySql = this.buildOrderBy(orderBy);
    const limitSql = this.buildLimit(limit);
    return sql`${withSql}update ${table2} set ${setSql}${fromSql}${joinsSql}${whereSql}${returningSql}${orderBySql}${limitSql}`;
  }
  /**
   * Builds selection SQL with provided fields/expressions
   *
   * Examples:
   *
   * `select <selection> from`
   *
   * `insert ... returning <selection>`
   *
   * If `isSingleTable` is true, then columns won't be prefixed with table name
   */
  buildSelection(fields, { isSingleTable = false } = {}) {
    const columnsLen = fields.length;
    const chunks = fields.flatMap(({ field }, i) => {
      const chunk = [];
      if (is(field, SQL.Aliased) && field.isSelectionField) {
        chunk.push(sql.identifier(field.fieldAlias));
      } else if (is(field, SQL.Aliased) || is(field, SQL)) {
        const query = is(field, SQL.Aliased) ? field.sql : field;
        if (isSingleTable) {
          chunk.push(
            new SQL(
              query.queryChunks.map((c) => {
                if (is(c, Column)) {
                  return sql.identifier(this.casing.getColumnCasing(c));
                }
                return c;
              })
            )
          );
        } else {
          chunk.push(query);
        }
        if (is(field, SQL.Aliased)) {
          chunk.push(sql` as ${sql.identifier(field.fieldAlias)}`);
        }
      } else if (is(field, Column)) {
        const tableName = field.table[Table.Symbol.Name];
        if (field.columnType === "SQLiteNumericBigInt") {
          if (isSingleTable) {
            chunk.push(
              sql`cast(${sql.identifier(this.casing.getColumnCasing(field))} as text)`
            );
          } else {
            chunk.push(
              sql`cast(${sql.identifier(tableName)}.${sql.identifier(this.casing.getColumnCasing(field))} as text)`
            );
          }
        } else {
          if (isSingleTable) {
            chunk.push(sql.identifier(this.casing.getColumnCasing(field)));
          } else {
            chunk.push(
              sql`${sql.identifier(tableName)}.${sql.identifier(this.casing.getColumnCasing(field))}`
            );
          }
        }
      } else if (is(field, Subquery)) {
        const entries = Object.entries(field._.selectedFields);
        if (entries.length === 1) {
          const entry = entries[0][1];
          const fieldDecoder = is(entry, SQL) ? entry.decoder : is(entry, Column) ? { mapFromDriverValue: (v) => entry.mapFromDriverValue(v) } : entry.sql.decoder;
          if (fieldDecoder) field._.sql.decoder = fieldDecoder;
        }
        chunk.push(field);
      }
      if (i < columnsLen - 1) {
        chunk.push(sql`, `);
      }
      return chunk;
    });
    return sql.join(chunks);
  }
  buildJoins(joins) {
    if (!joins || joins.length === 0) {
      return void 0;
    }
    const joinsArray = [];
    if (joins) {
      for (const [index, joinMeta] of joins.entries()) {
        if (index === 0) {
          joinsArray.push(sql` `);
        }
        const table2 = joinMeta.table;
        const onSql = joinMeta.on ? sql` on ${joinMeta.on}` : void 0;
        if (is(table2, SQLiteTable)) {
          const tableName = table2[SQLiteTable.Symbol.Name];
          const tableSchema = table2[SQLiteTable.Symbol.Schema];
          const origTableName = table2[SQLiteTable.Symbol.OriginalName];
          const alias = tableName === origTableName ? void 0 : joinMeta.alias;
          joinsArray.push(
            sql`${sql.raw(joinMeta.joinType)} join ${tableSchema ? sql`${sql.identifier(tableSchema)}.` : void 0}${sql.identifier(
              origTableName
            )}${alias && sql` ${sql.identifier(alias)}`}${onSql}`
          );
        } else {
          joinsArray.push(
            sql`${sql.raw(joinMeta.joinType)} join ${table2}${onSql}`
          );
        }
        if (index < joins.length - 1) {
          joinsArray.push(sql` `);
        }
      }
    }
    return sql.join(joinsArray);
  }
  buildLimit(limit) {
    return typeof limit === "object" || typeof limit === "number" && limit >= 0 ? sql` limit ${limit}` : void 0;
  }
  buildOrderBy(orderBy) {
    const orderByList = [];
    if (orderBy) {
      for (const [index, orderByValue] of orderBy.entries()) {
        orderByList.push(orderByValue);
        if (index < orderBy.length - 1) {
          orderByList.push(sql`, `);
        }
      }
    }
    return orderByList.length > 0 ? sql` order by ${sql.join(orderByList)}` : void 0;
  }
  buildFromTable(table2) {
    if (is(table2, Table) && table2[Table.Symbol.IsAlias]) {
      return sql`${sql`${sql.identifier(table2[Table.Symbol.Schema] ?? "")}.`.if(table2[Table.Symbol.Schema])}${sql.identifier(
        table2[Table.Symbol.OriginalName]
      )} ${sql.identifier(table2[Table.Symbol.Name])}`;
    }
    return table2;
  }
  buildSelectQuery({
    withList,
    fields,
    fieldsFlat,
    where,
    having,
    table: table2,
    joins,
    orderBy,
    groupBy,
    limit,
    offset,
    distinct,
    setOperators
  }) {
    const fieldsList = fieldsFlat ?? orderSelectedFields(fields);
    for (const f of fieldsList) {
      if (is(f.field, Column) && getTableName(f.field.table) !== (is(table2, Subquery) ? table2._.alias : is(table2, SQLiteViewBase) ? table2[ViewBaseConfig].name : is(table2, SQL) ? void 0 : getTableName(table2)) && !((table22) => joins?.some(
        ({ alias }) => alias === (table22[Table.Symbol.IsAlias] ? getTableName(table22) : table22[Table.Symbol.BaseName])
      ))(f.field.table)) {
        const tableName = getTableName(f.field.table);
        throw new Error(
          `Your "${f.path.join(
            "->"
          )}" field references a column "${tableName}"."${f.field.name}", but the table "${tableName}" is not part of the query! Did you forget to join it?`
        );
      }
    }
    const isSingleTable = !joins || joins.length === 0;
    const withSql = this.buildWithCTE(withList);
    const distinctSql = distinct ? sql` distinct` : void 0;
    const selection = this.buildSelection(fieldsList, { isSingleTable });
    const tableSql = this.buildFromTable(table2);
    const joinsSql = this.buildJoins(joins);
    const whereSql = where ? sql` where ${where}` : void 0;
    const havingSql = having ? sql` having ${having}` : void 0;
    const groupByList = [];
    if (groupBy) {
      for (const [index, groupByValue] of groupBy.entries()) {
        groupByList.push(groupByValue);
        if (index < groupBy.length - 1) {
          groupByList.push(sql`, `);
        }
      }
    }
    const groupBySql = groupByList.length > 0 ? sql` group by ${sql.join(groupByList)}` : void 0;
    const orderBySql = this.buildOrderBy(orderBy);
    const limitSql = this.buildLimit(limit);
    const offsetSql = offset ? sql` offset ${offset}` : void 0;
    const finalQuery = sql`${withSql}select${distinctSql} ${selection} from ${tableSql}${joinsSql}${whereSql}${groupBySql}${havingSql}${orderBySql}${limitSql}${offsetSql}`;
    if (setOperators.length > 0) {
      return this.buildSetOperations(finalQuery, setOperators);
    }
    return finalQuery;
  }
  buildSetOperations(leftSelect, setOperators) {
    const [setOperator, ...rest] = setOperators;
    if (!setOperator) {
      throw new Error("Cannot pass undefined values to any set operator");
    }
    if (rest.length === 0) {
      return this.buildSetOperationQuery({ leftSelect, setOperator });
    }
    return this.buildSetOperations(
      this.buildSetOperationQuery({ leftSelect, setOperator }),
      rest
    );
  }
  buildSetOperationQuery({
    leftSelect,
    setOperator: { type, isAll, rightSelect, limit, orderBy, offset }
  }) {
    const leftChunk = sql`${leftSelect.getSQL()} `;
    const rightChunk = sql`${rightSelect.getSQL()}`;
    let orderBySql;
    if (orderBy && orderBy.length > 0) {
      const orderByValues = [];
      for (const singleOrderBy of orderBy) {
        if (is(singleOrderBy, SQLiteColumn)) {
          orderByValues.push(sql.identifier(singleOrderBy.name));
        } else if (is(singleOrderBy, SQL)) {
          for (let i = 0; i < singleOrderBy.queryChunks.length; i++) {
            const chunk = singleOrderBy.queryChunks[i];
            if (is(chunk, SQLiteColumn)) {
              singleOrderBy.queryChunks[i] = sql.identifier(
                this.casing.getColumnCasing(chunk)
              );
            }
          }
          orderByValues.push(sql`${singleOrderBy}`);
        } else {
          orderByValues.push(sql`${singleOrderBy}`);
        }
      }
      orderBySql = sql` order by ${sql.join(orderByValues, sql`, `)}`;
    }
    const limitSql = typeof limit === "object" || typeof limit === "number" && limit >= 0 ? sql` limit ${limit}` : void 0;
    const operatorChunk = sql.raw(`${type} ${isAll ? "all " : ""}`);
    const offsetSql = offset ? sql` offset ${offset}` : void 0;
    return sql`${leftChunk}${operatorChunk}${rightChunk}${orderBySql}${limitSql}${offsetSql}`;
  }
  buildInsertQuery({
    table: table2,
    values: valuesOrSelect,
    onConflict,
    returning,
    withList,
    select
  }) {
    const valuesSqlList = [];
    const columns = table2[Table.Symbol.Columns];
    const colEntries = Object.entries(columns).filter(
      ([_, col]) => !col.shouldDisableInsert()
    );
    const insertOrder = colEntries.map(([, column]) => sql.identifier(this.casing.getColumnCasing(column)));
    if (select) {
      const select2 = valuesOrSelect;
      if (is(select2, SQL)) {
        valuesSqlList.push(select2);
      } else {
        valuesSqlList.push(select2.getSQL());
      }
    } else {
      const values = valuesOrSelect;
      valuesSqlList.push(sql.raw("values "));
      for (const [valueIndex, value] of values.entries()) {
        const valueList = [];
        for (const [fieldName, col] of colEntries) {
          const colValue = value[fieldName];
          if (colValue === void 0 || is(colValue, Param) && colValue.value === void 0) {
            let defaultValue;
            if (col.default !== null && col.default !== void 0) {
              defaultValue = is(col.default, SQL) ? col.default : sql.param(col.default, col);
            } else if (col.defaultFn !== void 0) {
              const defaultFnResult = col.defaultFn();
              defaultValue = is(defaultFnResult, SQL) ? defaultFnResult : sql.param(defaultFnResult, col);
            } else if (!col.default && col.onUpdateFn !== void 0) {
              const onUpdateFnResult = col.onUpdateFn();
              defaultValue = is(onUpdateFnResult, SQL) ? onUpdateFnResult : sql.param(onUpdateFnResult, col);
            } else {
              defaultValue = sql`null`;
            }
            valueList.push(defaultValue);
          } else {
            valueList.push(colValue);
          }
        }
        valuesSqlList.push(valueList);
        if (valueIndex < values.length - 1) {
          valuesSqlList.push(sql`, `);
        }
      }
    }
    const withSql = this.buildWithCTE(withList);
    const valuesSql = sql.join(valuesSqlList);
    const returningSql = returning ? sql` returning ${this.buildSelection(returning, { isSingleTable: true })}` : void 0;
    const onConflictSql = onConflict?.length ? sql.join(onConflict) : void 0;
    return sql`${withSql}insert into ${table2} ${insertOrder} ${valuesSql}${onConflictSql}${returningSql}`;
  }
  sqlToQuery(sql2, invokeSource) {
    return sql2.toQuery({
      casing: this.casing,
      escapeName: this.escapeName,
      escapeParam: this.escapeParam,
      escapeString: this.escapeString,
      invokeSource
    });
  }
  buildRelationalQuery({
    fullSchema,
    schema,
    tableNamesMap,
    table: table2,
    tableConfig,
    queryConfig: config,
    tableAlias,
    nestedQueryRelation,
    joinOn
  }) {
    let selection = [];
    let limit, offset, orderBy = [], where;
    const joins = [];
    if (config === true) {
      const selectionEntries = Object.entries(tableConfig.columns);
      selection = selectionEntries.map(([key, value]) => ({
        dbKey: value.name,
        tsKey: key,
        field: aliasedTableColumn(value, tableAlias),
        relationTableTsKey: void 0,
        isJson: false,
        selection: []
      }));
    } else {
      const aliasedColumns = Object.fromEntries(
        Object.entries(tableConfig.columns).map(([key, value]) => [
          key,
          aliasedTableColumn(value, tableAlias)
        ])
      );
      if (config.where) {
        const whereSql = typeof config.where === "function" ? config.where(aliasedColumns, getOperators()) : config.where;
        where = whereSql && mapColumnsInSQLToAlias(whereSql, tableAlias);
      }
      const fieldsSelection = [];
      let selectedColumns = [];
      if (config.columns) {
        let isIncludeMode = false;
        for (const [field, value] of Object.entries(config.columns)) {
          if (value === void 0) {
            continue;
          }
          if (field in tableConfig.columns) {
            if (!isIncludeMode && value === true) {
              isIncludeMode = true;
            }
            selectedColumns.push(field);
          }
        }
        if (selectedColumns.length > 0) {
          selectedColumns = isIncludeMode ? selectedColumns.filter((c) => config.columns?.[c] === true) : Object.keys(tableConfig.columns).filter(
            (key) => !selectedColumns.includes(key)
          );
        }
      } else {
        selectedColumns = Object.keys(tableConfig.columns);
      }
      for (const field of selectedColumns) {
        const column = tableConfig.columns[field];
        fieldsSelection.push({ tsKey: field, value: column });
      }
      let selectedRelations = [];
      if (config.with) {
        selectedRelations = Object.entries(config.with).filter(
          (entry) => !!entry[1]
        ).map(([tsKey, queryConfig]) => ({
          tsKey,
          queryConfig,
          relation: tableConfig.relations[tsKey]
        }));
      }
      let extras;
      if (config.extras) {
        extras = typeof config.extras === "function" ? config.extras(aliasedColumns, { sql }) : config.extras;
        for (const [tsKey, value] of Object.entries(extras)) {
          fieldsSelection.push({
            tsKey,
            value: mapColumnsInAliasedSQLToAlias(value, tableAlias)
          });
        }
      }
      for (const { tsKey, value } of fieldsSelection) {
        selection.push({
          dbKey: is(value, SQL.Aliased) ? value.fieldAlias : tableConfig.columns[tsKey].name,
          tsKey,
          field: is(value, Column) ? aliasedTableColumn(value, tableAlias) : value,
          relationTableTsKey: void 0,
          isJson: false,
          selection: []
        });
      }
      let orderByOrig = typeof config.orderBy === "function" ? config.orderBy(aliasedColumns, getOrderByOperators()) : config.orderBy ?? [];
      if (!Array.isArray(orderByOrig)) {
        orderByOrig = [orderByOrig];
      }
      orderBy = orderByOrig.map((orderByValue) => {
        if (is(orderByValue, Column)) {
          return aliasedTableColumn(orderByValue, tableAlias);
        }
        return mapColumnsInSQLToAlias(orderByValue, tableAlias);
      });
      limit = config.limit;
      offset = config.offset;
      for (const {
        tsKey: selectedRelationTsKey,
        queryConfig: selectedRelationConfigValue,
        relation
      } of selectedRelations) {
        const normalizedRelation = normalizeRelation(
          schema,
          tableNamesMap,
          relation
        );
        const relationTableName = getTableUniqueName(relation.referencedTable);
        const relationTableTsName = tableNamesMap[relationTableName];
        const relationTableAlias = `${tableAlias}_${selectedRelationTsKey}`;
        const joinOn2 = and(
          ...normalizedRelation.fields.map(
            (field2, i) => eq(
              aliasedTableColumn(
                normalizedRelation.references[i],
                relationTableAlias
              ),
              aliasedTableColumn(field2, tableAlias)
            )
          )
        );
        const builtRelation = this.buildRelationalQuery({
          fullSchema,
          schema,
          tableNamesMap,
          table: fullSchema[relationTableTsName],
          tableConfig: schema[relationTableTsName],
          queryConfig: is(relation, One) ? selectedRelationConfigValue === true ? { limit: 1 } : { ...selectedRelationConfigValue, limit: 1 } : selectedRelationConfigValue,
          tableAlias: relationTableAlias,
          joinOn: joinOn2,
          nestedQueryRelation: relation
        });
        const field = sql`(${builtRelation.sql})`.as(selectedRelationTsKey);
        selection.push({
          dbKey: selectedRelationTsKey,
          tsKey: selectedRelationTsKey,
          field,
          relationTableTsKey: relationTableTsName,
          isJson: true,
          selection: builtRelation.selection
        });
      }
    }
    if (selection.length === 0) {
      throw new DrizzleError({
        message: `No fields selected for table "${tableConfig.tsName}" ("${tableAlias}"). You need to have at least one item in "columns", "with" or "extras". If you need to select all columns, omit the "columns" key or set it to undefined.`
      });
    }
    let result;
    where = and(joinOn, where);
    if (nestedQueryRelation) {
      let field = sql`json_array(${sql.join(
        selection.map(
          ({ field: field2 }) => is(field2, SQLiteColumn) ? sql.identifier(this.casing.getColumnCasing(field2)) : is(field2, SQL.Aliased) ? field2.sql : field2
        ),
        sql`, `
      )})`;
      if (is(nestedQueryRelation, Many)) {
        field = sql`coalesce(json_group_array(${field}), json_array())`;
      }
      const nestedSelection = [
        {
          dbKey: "data",
          tsKey: "data",
          field: field.as("data"),
          isJson: true,
          relationTableTsKey: tableConfig.tsName,
          selection
        }
      ];
      const needsSubquery = limit !== void 0 || offset !== void 0 || orderBy.length > 0;
      if (needsSubquery) {
        result = this.buildSelectQuery({
          table: aliasedTable(table2, tableAlias),
          fields: {},
          fieldsFlat: [
            {
              path: [],
              field: sql.raw("*")
            }
          ],
          where,
          limit,
          offset,
          orderBy,
          setOperators: []
        });
        where = void 0;
        limit = void 0;
        offset = void 0;
        orderBy = void 0;
      } else {
        result = aliasedTable(table2, tableAlias);
      }
      result = this.buildSelectQuery({
        table: is(result, SQLiteTable) ? result : new Subquery(result, {}, tableAlias),
        fields: {},
        fieldsFlat: nestedSelection.map(({ field: field2 }) => ({
          path: [],
          field: is(field2, Column) ? aliasedTableColumn(field2, tableAlias) : field2
        })),
        joins,
        where,
        limit,
        offset,
        orderBy,
        setOperators: []
      });
    } else {
      result = this.buildSelectQuery({
        table: aliasedTable(table2, tableAlias),
        fields: {},
        fieldsFlat: selection.map(({ field }) => ({
          path: [],
          field: is(field, Column) ? aliasedTableColumn(field, tableAlias) : field
        })),
        joins,
        where,
        limit,
        offset,
        orderBy,
        setOperators: []
      });
    }
    return {
      tableTsKey: tableConfig.tsName,
      sql: result,
      selection
    };
  }
};
var SQLiteSyncDialect = class extends SQLiteDialect {
  static [entityKind] = "SQLiteSyncDialect";
  migrate(migrations, session, config) {
    const migrationsTable = config === void 0 ? "__drizzle_migrations" : typeof config === "string" ? "__drizzle_migrations" : config.migrationsTable ?? "__drizzle_migrations";
    const migrationTableCreate = sql`
			CREATE TABLE IF NOT EXISTS ${sql.identifier(migrationsTable)} (
				id SERIAL PRIMARY KEY,
				hash text NOT NULL,
				created_at numeric
			)
		`;
    session.run(migrationTableCreate);
    const dbMigrations = session.values(
      sql`SELECT id, hash, created_at FROM ${sql.identifier(migrationsTable)} ORDER BY created_at DESC LIMIT 1`
    );
    const lastDbMigration = dbMigrations[0] ?? void 0;
    session.run(sql`BEGIN`);
    try {
      for (const migration of migrations) {
        if (!lastDbMigration || Number(lastDbMigration[2]) < migration.folderMillis) {
          for (const stmt of migration.sql) {
            session.run(sql.raw(stmt));
          }
          session.run(
            sql`INSERT INTO ${sql.identifier(
              migrationsTable
            )} ("hash", "created_at") VALUES(${migration.hash}, ${migration.folderMillis})`
          );
        }
      }
      session.run(sql`COMMIT`);
    } catch (e) {
      session.run(sql`ROLLBACK`);
      throw e;
    }
  }
};
var SQLiteAsyncDialect = class extends SQLiteDialect {
  static [entityKind] = "SQLiteAsyncDialect";
  async migrate(migrations, session, config) {
    const migrationsTable = config === void 0 ? "__drizzle_migrations" : typeof config === "string" ? "__drizzle_migrations" : config.migrationsTable ?? "__drizzle_migrations";
    const migrationTableCreate = sql`
			CREATE TABLE IF NOT EXISTS ${sql.identifier(migrationsTable)} (
				id SERIAL PRIMARY KEY,
				hash text NOT NULL,
				created_at numeric
			)
		`;
    await session.run(migrationTableCreate);
    const dbMigrations = await session.values(
      sql`SELECT id, hash, created_at FROM ${sql.identifier(migrationsTable)} ORDER BY created_at DESC LIMIT 1`
    );
    const lastDbMigration = dbMigrations[0] ?? void 0;
    await session.transaction(async (tx) => {
      for (const migration of migrations) {
        if (!lastDbMigration || Number(lastDbMigration[2]) < migration.folderMillis) {
          for (const stmt of migration.sql) {
            await tx.run(sql.raw(stmt));
          }
          await tx.run(
            sql`INSERT INTO ${sql.identifier(
              migrationsTable
            )} ("hash", "created_at") VALUES(${migration.hash}, ${migration.folderMillis})`
          );
        }
      }
    });
  }
};

// node_modules/drizzle-orm/query-builders/query-builder.js
var TypedQueryBuilder = class {
  static [entityKind] = "TypedQueryBuilder";
  /** @internal */
  getSelectedFields() {
    return this._.selectedFields;
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/select.js
var SQLiteSelectBuilder = class {
  static [entityKind] = "SQLiteSelectBuilder";
  fields;
  session;
  dialect;
  withList;
  distinct;
  constructor(config) {
    this.fields = config.fields;
    this.session = config.session;
    this.dialect = config.dialect;
    this.withList = config.withList;
    this.distinct = config.distinct;
  }
  from(source) {
    const isPartialSelect = !!this.fields;
    let fields;
    if (this.fields) {
      fields = this.fields;
    } else if (is(source, Subquery)) {
      fields = Object.fromEntries(
        Object.keys(source._.selectedFields).map((key) => [key, source[key]])
      );
    } else if (is(source, SQLiteViewBase)) {
      fields = source[ViewBaseConfig].selectedFields;
    } else if (is(source, SQL)) {
      fields = {};
    } else {
      fields = getTableColumns(source);
    }
    return new SQLiteSelectBase({
      table: source,
      fields,
      isPartialSelect,
      session: this.session,
      dialect: this.dialect,
      withList: this.withList,
      distinct: this.distinct
    });
  }
};
var SQLiteSelectQueryBuilderBase = class extends TypedQueryBuilder {
  static [entityKind] = "SQLiteSelectQueryBuilder";
  _;
  /** @internal */
  config;
  joinsNotNullableMap;
  tableName;
  isPartialSelect;
  session;
  dialect;
  cacheConfig = void 0;
  usedTables = /* @__PURE__ */ new Set();
  constructor({ table: table2, fields, isPartialSelect, session, dialect, withList, distinct }) {
    super();
    this.config = {
      withList,
      table: table2,
      fields: { ...fields },
      distinct,
      setOperators: []
    };
    this.isPartialSelect = isPartialSelect;
    this.session = session;
    this.dialect = dialect;
    this._ = {
      selectedFields: fields,
      config: this.config
    };
    this.tableName = getTableLikeName(table2);
    this.joinsNotNullableMap = typeof this.tableName === "string" ? { [this.tableName]: true } : {};
    for (const item of extractUsedTable(table2)) this.usedTables.add(item);
  }
  /** @internal */
  getUsedTables() {
    return [...this.usedTables];
  }
  createJoin(joinType) {
    return (table2, on) => {
      const baseTableName = this.tableName;
      const tableName = getTableLikeName(table2);
      for (const item of extractUsedTable(table2)) this.usedTables.add(item);
      if (typeof tableName === "string" && this.config.joins?.some((join) => join.alias === tableName)) {
        throw new Error(`Alias "${tableName}" is already used in this query`);
      }
      if (!this.isPartialSelect) {
        if (Object.keys(this.joinsNotNullableMap).length === 1 && typeof baseTableName === "string") {
          this.config.fields = {
            [baseTableName]: this.config.fields
          };
        }
        if (typeof tableName === "string" && !is(table2, SQL)) {
          const selection = is(table2, Subquery) ? table2._.selectedFields : is(table2, View) ? table2[ViewBaseConfig].selectedFields : table2[Table.Symbol.Columns];
          this.config.fields[tableName] = selection;
        }
      }
      if (typeof on === "function") {
        on = on(
          new Proxy(
            this.config.fields,
            new SelectionProxyHandler({ sqlAliasedBehavior: "sql", sqlBehavior: "sql" })
          )
        );
      }
      if (!this.config.joins) {
        this.config.joins = [];
      }
      this.config.joins.push({ on, table: table2, joinType, alias: tableName });
      if (typeof tableName === "string") {
        switch (joinType) {
          case "left": {
            this.joinsNotNullableMap[tableName] = false;
            break;
          }
          case "right": {
            this.joinsNotNullableMap = Object.fromEntries(
              Object.entries(this.joinsNotNullableMap).map(([key]) => [key, false])
            );
            this.joinsNotNullableMap[tableName] = true;
            break;
          }
          case "cross":
          case "inner": {
            this.joinsNotNullableMap[tableName] = true;
            break;
          }
          case "full": {
            this.joinsNotNullableMap = Object.fromEntries(
              Object.entries(this.joinsNotNullableMap).map(([key]) => [key, false])
            );
            this.joinsNotNullableMap[tableName] = false;
            break;
          }
        }
      }
      return this;
    };
  }
  /**
   * Executes a `left join` operation by adding another table to the current query.
   *
   * Calling this method associates each row of the table with the corresponding row from the joined table, if a match is found. If no matching row exists, it sets all columns of the joined table to null.
   *
   * See docs: {@link https://orm.drizzle.team/docs/joins#left-join}
   *
   * @param table the table to join.
   * @param on the `on` clause.
   *
   * @example
   *
   * ```ts
   * // Select all users and their pets
   * const usersWithPets: { user: User; pets: Pet | null; }[] = await db.select()
   *   .from(users)
   *   .leftJoin(pets, eq(users.id, pets.ownerId))
   *
   * // Select userId and petId
   * const usersIdsAndPetIds: { userId: number; petId: number | null; }[] = await db.select({
   *   userId: users.id,
   *   petId: pets.id,
   * })
   *   .from(users)
   *   .leftJoin(pets, eq(users.id, pets.ownerId))
   * ```
   */
  leftJoin = this.createJoin("left");
  /**
   * Executes a `right join` operation by adding another table to the current query.
   *
   * Calling this method associates each row of the joined table with the corresponding row from the main table, if a match is found. If no matching row exists, it sets all columns of the main table to null.
   *
   * See docs: {@link https://orm.drizzle.team/docs/joins#right-join}
   *
   * @param table the table to join.
   * @param on the `on` clause.
   *
   * @example
   *
   * ```ts
   * // Select all users and their pets
   * const usersWithPets: { user: User | null; pets: Pet; }[] = await db.select()
   *   .from(users)
   *   .rightJoin(pets, eq(users.id, pets.ownerId))
   *
   * // Select userId and petId
   * const usersIdsAndPetIds: { userId: number | null; petId: number; }[] = await db.select({
   *   userId: users.id,
   *   petId: pets.id,
   * })
   *   .from(users)
   *   .rightJoin(pets, eq(users.id, pets.ownerId))
   * ```
   */
  rightJoin = this.createJoin("right");
  /**
   * Executes an `inner join` operation, creating a new table by combining rows from two tables that have matching values.
   *
   * Calling this method retrieves rows that have corresponding entries in both joined tables. Rows without matching entries in either table are excluded, resulting in a table that includes only matching pairs.
   *
   * See docs: {@link https://orm.drizzle.team/docs/joins#inner-join}
   *
   * @param table the table to join.
   * @param on the `on` clause.
   *
   * @example
   *
   * ```ts
   * // Select all users and their pets
   * const usersWithPets: { user: User; pets: Pet; }[] = await db.select()
   *   .from(users)
   *   .innerJoin(pets, eq(users.id, pets.ownerId))
   *
   * // Select userId and petId
   * const usersIdsAndPetIds: { userId: number; petId: number; }[] = await db.select({
   *   userId: users.id,
   *   petId: pets.id,
   * })
   *   .from(users)
   *   .innerJoin(pets, eq(users.id, pets.ownerId))
   * ```
   */
  innerJoin = this.createJoin("inner");
  /**
   * Executes a `full join` operation by combining rows from two tables into a new table.
   *
   * Calling this method retrieves all rows from both main and joined tables, merging rows with matching values and filling in `null` for non-matching columns.
   *
   * See docs: {@link https://orm.drizzle.team/docs/joins#full-join}
   *
   * @param table the table to join.
   * @param on the `on` clause.
   *
   * @example
   *
   * ```ts
   * // Select all users and their pets
   * const usersWithPets: { user: User | null; pets: Pet | null; }[] = await db.select()
   *   .from(users)
   *   .fullJoin(pets, eq(users.id, pets.ownerId))
   *
   * // Select userId and petId
   * const usersIdsAndPetIds: { userId: number | null; petId: number | null; }[] = await db.select({
   *   userId: users.id,
   *   petId: pets.id,
   * })
   *   .from(users)
   *   .fullJoin(pets, eq(users.id, pets.ownerId))
   * ```
   */
  fullJoin = this.createJoin("full");
  /**
   * Executes a `cross join` operation by combining rows from two tables into a new table.
   *
   * Calling this method retrieves all rows from both main and joined tables, merging all rows from each table.
   *
   * See docs: {@link https://orm.drizzle.team/docs/joins#cross-join}
   *
   * @param table the table to join.
   *
   * @example
   *
   * ```ts
   * // Select all users, each user with every pet
   * const usersWithPets: { user: User; pets: Pet; }[] = await db.select()
   *   .from(users)
   *   .crossJoin(pets)
   *
   * // Select userId and petId
   * const usersIdsAndPetIds: { userId: number; petId: number; }[] = await db.select({
   *   userId: users.id,
   *   petId: pets.id,
   * })
   *   .from(users)
   *   .crossJoin(pets)
   * ```
   */
  crossJoin = this.createJoin("cross");
  createSetOperator(type, isAll) {
    return (rightSelection) => {
      const rightSelect = typeof rightSelection === "function" ? rightSelection(getSQLiteSetOperators()) : rightSelection;
      if (!haveSameKeys(this.getSelectedFields(), rightSelect.getSelectedFields())) {
        throw new Error(
          "Set operator error (union / intersect / except): selected fields are not the same or are in a different order"
        );
      }
      this.config.setOperators.push({ type, isAll, rightSelect });
      return this;
    };
  }
  /**
   * Adds `union` set operator to the query.
   *
   * Calling this method will combine the result sets of the `select` statements and remove any duplicate rows that appear across them.
   *
   * See docs: {@link https://orm.drizzle.team/docs/set-operations#union}
   *
   * @example
   *
   * ```ts
   * // Select all unique names from customers and users tables
   * await db.select({ name: users.name })
   *   .from(users)
   *   .union(
   *     db.select({ name: customers.name }).from(customers)
   *   );
   * // or
   * import { union } from 'drizzle-orm/sqlite-core'
   *
   * await union(
   *   db.select({ name: users.name }).from(users),
   *   db.select({ name: customers.name }).from(customers)
   * );
   * ```
   */
  union = this.createSetOperator("union", false);
  /**
   * Adds `union all` set operator to the query.
   *
   * Calling this method will combine the result-set of the `select` statements and keep all duplicate rows that appear across them.
   *
   * See docs: {@link https://orm.drizzle.team/docs/set-operations#union-all}
   *
   * @example
   *
   * ```ts
   * // Select all transaction ids from both online and in-store sales
   * await db.select({ transaction: onlineSales.transactionId })
   *   .from(onlineSales)
   *   .unionAll(
   *     db.select({ transaction: inStoreSales.transactionId }).from(inStoreSales)
   *   );
   * // or
   * import { unionAll } from 'drizzle-orm/sqlite-core'
   *
   * await unionAll(
   *   db.select({ transaction: onlineSales.transactionId }).from(onlineSales),
   *   db.select({ transaction: inStoreSales.transactionId }).from(inStoreSales)
   * );
   * ```
   */
  unionAll = this.createSetOperator("union", true);
  /**
   * Adds `intersect` set operator to the query.
   *
   * Calling this method will retain only the rows that are present in both result sets and eliminate duplicates.
   *
   * See docs: {@link https://orm.drizzle.team/docs/set-operations#intersect}
   *
   * @example
   *
   * ```ts
   * // Select course names that are offered in both departments A and B
   * await db.select({ courseName: depA.courseName })
   *   .from(depA)
   *   .intersect(
   *     db.select({ courseName: depB.courseName }).from(depB)
   *   );
   * // or
   * import { intersect } from 'drizzle-orm/sqlite-core'
   *
   * await intersect(
   *   db.select({ courseName: depA.courseName }).from(depA),
   *   db.select({ courseName: depB.courseName }).from(depB)
   * );
   * ```
   */
  intersect = this.createSetOperator("intersect", false);
  /**
   * Adds `except` set operator to the query.
   *
   * Calling this method will retrieve all unique rows from the left query, except for the rows that are present in the result set of the right query.
   *
   * See docs: {@link https://orm.drizzle.team/docs/set-operations#except}
   *
   * @example
   *
   * ```ts
   * // Select all courses offered in department A but not in department B
   * await db.select({ courseName: depA.courseName })
   *   .from(depA)
   *   .except(
   *     db.select({ courseName: depB.courseName }).from(depB)
   *   );
   * // or
   * import { except } from 'drizzle-orm/sqlite-core'
   *
   * await except(
   *   db.select({ courseName: depA.courseName }).from(depA),
   *   db.select({ courseName: depB.courseName }).from(depB)
   * );
   * ```
   */
  except = this.createSetOperator("except", false);
  /** @internal */
  addSetOperators(setOperators) {
    this.config.setOperators.push(...setOperators);
    return this;
  }
  /**
   * Adds a `where` clause to the query.
   *
   * Calling this method will select only those rows that fulfill a specified condition.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#filtering}
   *
   * @param where the `where` clause.
   *
   * @example
   * You can use conditional operators and `sql function` to filter the rows to be selected.
   *
   * ```ts
   * // Select all cars with green color
   * await db.select().from(cars).where(eq(cars.color, 'green'));
   * // or
   * await db.select().from(cars).where(sql`${cars.color} = 'green'`)
   * ```
   *
   * You can logically combine conditional operators with `and()` and `or()` operators:
   *
   * ```ts
   * // Select all BMW cars with a green color
   * await db.select().from(cars).where(and(eq(cars.color, 'green'), eq(cars.brand, 'BMW')));
   *
   * // Select all cars with the green or blue color
   * await db.select().from(cars).where(or(eq(cars.color, 'green'), eq(cars.color, 'blue')));
   * ```
   */
  where(where) {
    if (typeof where === "function") {
      where = where(
        new Proxy(
          this.config.fields,
          new SelectionProxyHandler({ sqlAliasedBehavior: "sql", sqlBehavior: "sql" })
        )
      );
    }
    this.config.where = where;
    return this;
  }
  /**
   * Adds a `having` clause to the query.
   *
   * Calling this method will select only those rows that fulfill a specified condition. It is typically used with aggregate functions to filter the aggregated data based on a specified condition.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#aggregations}
   *
   * @param having the `having` clause.
   *
   * @example
   *
   * ```ts
   * // Select all brands with more than one car
   * await db.select({
   * 	brand: cars.brand,
   * 	count: sql<number>`cast(count(${cars.id}) as int)`,
   * })
   *   .from(cars)
   *   .groupBy(cars.brand)
   *   .having(({ count }) => gt(count, 1));
   * ```
   */
  having(having) {
    if (typeof having === "function") {
      having = having(
        new Proxy(
          this.config.fields,
          new SelectionProxyHandler({ sqlAliasedBehavior: "sql", sqlBehavior: "sql" })
        )
      );
    }
    this.config.having = having;
    return this;
  }
  groupBy(...columns) {
    if (typeof columns[0] === "function") {
      const groupBy = columns[0](
        new Proxy(
          this.config.fields,
          new SelectionProxyHandler({ sqlAliasedBehavior: "alias", sqlBehavior: "sql" })
        )
      );
      this.config.groupBy = Array.isArray(groupBy) ? groupBy : [groupBy];
    } else {
      this.config.groupBy = columns;
    }
    return this;
  }
  orderBy(...columns) {
    if (typeof columns[0] === "function") {
      const orderBy = columns[0](
        new Proxy(
          this.config.fields,
          new SelectionProxyHandler({ sqlAliasedBehavior: "alias", sqlBehavior: "sql" })
        )
      );
      const orderByArray = Array.isArray(orderBy) ? orderBy : [orderBy];
      if (this.config.setOperators.length > 0) {
        this.config.setOperators.at(-1).orderBy = orderByArray;
      } else {
        this.config.orderBy = orderByArray;
      }
    } else {
      const orderByArray = columns;
      if (this.config.setOperators.length > 0) {
        this.config.setOperators.at(-1).orderBy = orderByArray;
      } else {
        this.config.orderBy = orderByArray;
      }
    }
    return this;
  }
  /**
   * Adds a `limit` clause to the query.
   *
   * Calling this method will set the maximum number of rows that will be returned by this query.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#limit--offset}
   *
   * @param limit the `limit` clause.
   *
   * @example
   *
   * ```ts
   * // Get the first 10 people from this query.
   * await db.select().from(people).limit(10);
   * ```
   */
  limit(limit) {
    if (this.config.setOperators.length > 0) {
      this.config.setOperators.at(-1).limit = limit;
    } else {
      this.config.limit = limit;
    }
    return this;
  }
  /**
   * Adds an `offset` clause to the query.
   *
   * Calling this method will skip a number of rows when returning results from this query.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#limit--offset}
   *
   * @param offset the `offset` clause.
   *
   * @example
   *
   * ```ts
   * // Get the 10th-20th people from this query.
   * await db.select().from(people).offset(10).limit(10);
   * ```
   */
  offset(offset) {
    if (this.config.setOperators.length > 0) {
      this.config.setOperators.at(-1).offset = offset;
    } else {
      this.config.offset = offset;
    }
    return this;
  }
  /** @internal */
  getSQL() {
    return this.dialect.buildSelectQuery(this.config);
  }
  toSQL() {
    const { typings: _typings, ...rest } = this.dialect.sqlToQuery(this.getSQL());
    return rest;
  }
  as(alias) {
    const usedTables = [];
    usedTables.push(...extractUsedTable(this.config.table));
    if (this.config.joins) {
      for (const it of this.config.joins) usedTables.push(...extractUsedTable(it.table));
    }
    return new Proxy(
      new Subquery(this.getSQL(), this.config.fields, alias, false, [...new Set(usedTables)]),
      new SelectionProxyHandler({ alias, sqlAliasedBehavior: "alias", sqlBehavior: "error" })
    );
  }
  /** @internal */
  getSelectedFields() {
    return new Proxy(
      this.config.fields,
      new SelectionProxyHandler({ alias: this.tableName, sqlAliasedBehavior: "alias", sqlBehavior: "error" })
    );
  }
  $dynamic() {
    return this;
  }
};
var SQLiteSelectBase = class extends SQLiteSelectQueryBuilderBase {
  static [entityKind] = "SQLiteSelect";
  /** @internal */
  _prepare(isOneTimeQuery = true) {
    if (!this.session) {
      throw new Error("Cannot execute a query on a query builder. Please use a database instance instead.");
    }
    const fieldsList = orderSelectedFields(this.config.fields);
    const query = this.session[isOneTimeQuery ? "prepareOneTimeQuery" : "prepareQuery"](
      this.dialect.sqlToQuery(this.getSQL()),
      fieldsList,
      "all",
      true,
      void 0,
      {
        type: "select",
        tables: [...this.usedTables]
      },
      this.cacheConfig
    );
    query.joinsNotNullableMap = this.joinsNotNullableMap;
    return query;
  }
  $withCache(config) {
    this.cacheConfig = config === void 0 ? { config: {}, enable: true, autoInvalidate: true } : config === false ? { enable: false } : { enable: true, autoInvalidate: true, ...config };
    return this;
  }
  prepare() {
    return this._prepare(false);
  }
  run = (placeholderValues) => {
    return this._prepare().run(placeholderValues);
  };
  all = (placeholderValues) => {
    return this._prepare().all(placeholderValues);
  };
  get = (placeholderValues) => {
    return this._prepare().get(placeholderValues);
  };
  values = (placeholderValues) => {
    return this._prepare().values(placeholderValues);
  };
  async execute() {
    return this.all();
  }
};
applyMixins(SQLiteSelectBase, [QueryPromise]);
function createSetOperator(type, isAll) {
  return (leftSelect, rightSelect, ...restSelects) => {
    const setOperators = [rightSelect, ...restSelects].map((select) => ({
      type,
      isAll,
      rightSelect: select
    }));
    for (const setOperator of setOperators) {
      if (!haveSameKeys(leftSelect.getSelectedFields(), setOperator.rightSelect.getSelectedFields())) {
        throw new Error(
          "Set operator error (union / intersect / except): selected fields are not the same or are in a different order"
        );
      }
    }
    return leftSelect.addSetOperators(setOperators);
  };
}
var getSQLiteSetOperators = () => ({
  union,
  unionAll,
  intersect,
  except
});
var union = createSetOperator("union", false);
var unionAll = createSetOperator("union", true);
var intersect = createSetOperator("intersect", false);
var except = createSetOperator("except", false);

// node_modules/drizzle-orm/sqlite-core/query-builders/query-builder.js
var QueryBuilder = class {
  static [entityKind] = "SQLiteQueryBuilder";
  dialect;
  dialectConfig;
  constructor(dialect) {
    this.dialect = is(dialect, SQLiteDialect) ? dialect : void 0;
    this.dialectConfig = is(dialect, SQLiteDialect) ? void 0 : dialect;
  }
  $with = (alias, selection) => {
    const queryBuilder = this;
    const as = (qb) => {
      if (typeof qb === "function") {
        qb = qb(queryBuilder);
      }
      return new Proxy(
        new WithSubquery(
          qb.getSQL(),
          selection ?? ("getSelectedFields" in qb ? qb.getSelectedFields() ?? {} : {}),
          alias,
          true
        ),
        new SelectionProxyHandler({ alias, sqlAliasedBehavior: "alias", sqlBehavior: "error" })
      );
    };
    return { as };
  };
  with(...queries) {
    const self = this;
    function select(fields) {
      return new SQLiteSelectBuilder({
        fields: fields ?? void 0,
        session: void 0,
        dialect: self.getDialect(),
        withList: queries
      });
    }
    function selectDistinct(fields) {
      return new SQLiteSelectBuilder({
        fields: fields ?? void 0,
        session: void 0,
        dialect: self.getDialect(),
        withList: queries,
        distinct: true
      });
    }
    return { select, selectDistinct };
  }
  select(fields) {
    return new SQLiteSelectBuilder({ fields: fields ?? void 0, session: void 0, dialect: this.getDialect() });
  }
  selectDistinct(fields) {
    return new SQLiteSelectBuilder({
      fields: fields ?? void 0,
      session: void 0,
      dialect: this.getDialect(),
      distinct: true
    });
  }
  // Lazy load dialect to avoid circular dependency
  getDialect() {
    if (!this.dialect) {
      this.dialect = new SQLiteSyncDialect(this.dialectConfig);
    }
    return this.dialect;
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/insert.js
var SQLiteInsertBuilder = class {
  constructor(table2, session, dialect, withList) {
    this.table = table2;
    this.session = session;
    this.dialect = dialect;
    this.withList = withList;
  }
  static [entityKind] = "SQLiteInsertBuilder";
  values(values) {
    values = Array.isArray(values) ? values : [values];
    if (values.length === 0) {
      throw new Error("values() must be called with at least one value");
    }
    const mappedValues = values.map((entry) => {
      const result = {};
      const cols = this.table[Table.Symbol.Columns];
      for (const colKey of Object.keys(entry)) {
        const colValue = entry[colKey];
        result[colKey] = is(colValue, SQL) ? colValue : new Param(colValue, cols[colKey]);
      }
      return result;
    });
    return new SQLiteInsertBase(this.table, mappedValues, this.session, this.dialect, this.withList);
  }
  select(selectQuery) {
    const select = typeof selectQuery === "function" ? selectQuery(new QueryBuilder()) : selectQuery;
    if (!is(select, SQL) && !haveSameKeys(this.table[Columns], select._.selectedFields)) {
      throw new Error(
        "Insert select error: selected fields are not the same or are in a different order compared to the table definition"
      );
    }
    return new SQLiteInsertBase(this.table, select, this.session, this.dialect, this.withList, true);
  }
};
var SQLiteInsertBase = class extends QueryPromise {
  constructor(table2, values, session, dialect, withList, select) {
    super();
    this.session = session;
    this.dialect = dialect;
    this.config = { table: table2, values, withList, select };
  }
  static [entityKind] = "SQLiteInsert";
  /** @internal */
  config;
  returning(fields = this.config.table[SQLiteTable.Symbol.Columns]) {
    this.config.returning = orderSelectedFields(fields);
    return this;
  }
  /**
   * Adds an `on conflict do nothing` clause to the query.
   *
   * Calling this method simply avoids inserting a row as its alternative action.
   *
   * See docs: {@link https://orm.drizzle.team/docs/insert#on-conflict-do-nothing}
   *
   * @param config The `target` and `where` clauses.
   *
   * @example
   * ```ts
   * // Insert one row and cancel the insert if there's a conflict
   * await db.insert(cars)
   *   .values({ id: 1, brand: 'BMW' })
   *   .onConflictDoNothing();
   *
   * // Explicitly specify conflict target
   * await db.insert(cars)
   *   .values({ id: 1, brand: 'BMW' })
   *   .onConflictDoNothing({ target: cars.id });
   * ```
   */
  onConflictDoNothing(config = {}) {
    if (!this.config.onConflict) this.config.onConflict = [];
    if (config.target === void 0) {
      this.config.onConflict.push(sql` on conflict do nothing`);
    } else {
      const targetSql = Array.isArray(config.target) ? sql`${config.target}` : sql`${[config.target]}`;
      const whereSql = config.where ? sql` where ${config.where}` : sql``;
      this.config.onConflict.push(sql` on conflict ${targetSql} do nothing${whereSql}`);
    }
    return this;
  }
  /**
   * Adds an `on conflict do update` clause to the query.
   *
   * Calling this method will update the existing row that conflicts with the row proposed for insertion as its alternative action.
   *
   * See docs: {@link https://orm.drizzle.team/docs/insert#upserts-and-conflicts}
   *
   * @param config The `target`, `set` and `where` clauses.
   *
   * @example
   * ```ts
   * // Update the row if there's a conflict
   * await db.insert(cars)
   *   .values({ id: 1, brand: 'BMW' })
   *   .onConflictDoUpdate({
   *     target: cars.id,
   *     set: { brand: 'Porsche' }
   *   });
   *
   * // Upsert with 'where' clause
   * await db.insert(cars)
   *   .values({ id: 1, brand: 'BMW' })
   *   .onConflictDoUpdate({
   *     target: cars.id,
   *     set: { brand: 'newBMW' },
   *     where: sql`${cars.createdAt} > '2023-01-01'::date`,
   *   });
   * ```
   */
  onConflictDoUpdate(config) {
    if (config.where && (config.targetWhere || config.setWhere)) {
      throw new Error(
        'You cannot use both "where" and "targetWhere"/"setWhere" at the same time - "where" is deprecated, use "targetWhere" or "setWhere" instead.'
      );
    }
    if (!this.config.onConflict) this.config.onConflict = [];
    const whereSql = config.where ? sql` where ${config.where}` : void 0;
    const targetWhereSql = config.targetWhere ? sql` where ${config.targetWhere}` : void 0;
    const setWhereSql = config.setWhere ? sql` where ${config.setWhere}` : void 0;
    const targetSql = Array.isArray(config.target) ? sql`${config.target}` : sql`${[config.target]}`;
    const setSql = this.dialect.buildUpdateSet(this.config.table, mapUpdateSet(this.config.table, config.set));
    this.config.onConflict.push(
      sql` on conflict ${targetSql}${targetWhereSql} do update set ${setSql}${whereSql}${setWhereSql}`
    );
    return this;
  }
  /** @internal */
  getSQL() {
    return this.dialect.buildInsertQuery(this.config);
  }
  toSQL() {
    const { typings: _typings, ...rest } = this.dialect.sqlToQuery(this.getSQL());
    return rest;
  }
  /** @internal */
  _prepare(isOneTimeQuery = true) {
    return this.session[isOneTimeQuery ? "prepareOneTimeQuery" : "prepareQuery"](
      this.dialect.sqlToQuery(this.getSQL()),
      this.config.returning,
      this.config.returning ? "all" : "run",
      true,
      void 0,
      {
        type: "insert",
        tables: extractUsedTable(this.config.table)
      }
    );
  }
  prepare() {
    return this._prepare(false);
  }
  run = (placeholderValues) => {
    return this._prepare().run(placeholderValues);
  };
  all = (placeholderValues) => {
    return this._prepare().all(placeholderValues);
  };
  get = (placeholderValues) => {
    return this._prepare().get(placeholderValues);
  };
  values = (placeholderValues) => {
    return this._prepare().values(placeholderValues);
  };
  async execute() {
    return this.config.returning ? this.all() : this.run();
  }
  $dynamic() {
    return this;
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/update.js
var SQLiteUpdateBuilder = class {
  constructor(table2, session, dialect, withList) {
    this.table = table2;
    this.session = session;
    this.dialect = dialect;
    this.withList = withList;
  }
  static [entityKind] = "SQLiteUpdateBuilder";
  set(values) {
    return new SQLiteUpdateBase(
      this.table,
      mapUpdateSet(this.table, values),
      this.session,
      this.dialect,
      this.withList
    );
  }
};
var SQLiteUpdateBase = class extends QueryPromise {
  constructor(table2, set, session, dialect, withList) {
    super();
    this.session = session;
    this.dialect = dialect;
    this.config = { set, table: table2, withList, joins: [] };
  }
  static [entityKind] = "SQLiteUpdate";
  /** @internal */
  config;
  from(source) {
    this.config.from = source;
    return this;
  }
  createJoin(joinType) {
    return (table2, on) => {
      const tableName = getTableLikeName(table2);
      if (typeof tableName === "string" && this.config.joins.some((join) => join.alias === tableName)) {
        throw new Error(`Alias "${tableName}" is already used in this query`);
      }
      if (typeof on === "function") {
        const from = this.config.from ? is(table2, SQLiteTable) ? table2[Table.Symbol.Columns] : is(table2, Subquery) ? table2._.selectedFields : is(table2, SQLiteViewBase) ? table2[ViewBaseConfig].selectedFields : void 0 : void 0;
        on = on(
          new Proxy(
            this.config.table[Table.Symbol.Columns],
            new SelectionProxyHandler({ sqlAliasedBehavior: "sql", sqlBehavior: "sql" })
          ),
          from && new Proxy(
            from,
            new SelectionProxyHandler({ sqlAliasedBehavior: "sql", sqlBehavior: "sql" })
          )
        );
      }
      this.config.joins.push({ on, table: table2, joinType, alias: tableName });
      return this;
    };
  }
  leftJoin = this.createJoin("left");
  rightJoin = this.createJoin("right");
  innerJoin = this.createJoin("inner");
  fullJoin = this.createJoin("full");
  /**
   * Adds a 'where' clause to the query.
   *
   * Calling this method will update only those rows that fulfill a specified condition.
   *
   * See docs: {@link https://orm.drizzle.team/docs/update}
   *
   * @param where the 'where' clause.
   *
   * @example
   * You can use conditional operators and `sql function` to filter the rows to be updated.
   *
   * ```ts
   * // Update all cars with green color
   * db.update(cars).set({ color: 'red' })
   *   .where(eq(cars.color, 'green'));
   * // or
   * db.update(cars).set({ color: 'red' })
   *   .where(sql`${cars.color} = 'green'`)
   * ```
   *
   * You can logically combine conditional operators with `and()` and `or()` operators:
   *
   * ```ts
   * // Update all BMW cars with a green color
   * db.update(cars).set({ color: 'red' })
   *   .where(and(eq(cars.color, 'green'), eq(cars.brand, 'BMW')));
   *
   * // Update all cars with the green or blue color
   * db.update(cars).set({ color: 'red' })
   *   .where(or(eq(cars.color, 'green'), eq(cars.color, 'blue')));
   * ```
   */
  where(where) {
    this.config.where = where;
    return this;
  }
  orderBy(...columns) {
    if (typeof columns[0] === "function") {
      const orderBy = columns[0](
        new Proxy(
          this.config.table[Table.Symbol.Columns],
          new SelectionProxyHandler({ sqlAliasedBehavior: "alias", sqlBehavior: "sql" })
        )
      );
      const orderByArray = Array.isArray(orderBy) ? orderBy : [orderBy];
      this.config.orderBy = orderByArray;
    } else {
      const orderByArray = columns;
      this.config.orderBy = orderByArray;
    }
    return this;
  }
  limit(limit) {
    this.config.limit = limit;
    return this;
  }
  returning(fields = this.config.table[SQLiteTable.Symbol.Columns]) {
    this.config.returning = orderSelectedFields(fields);
    return this;
  }
  /** @internal */
  getSQL() {
    return this.dialect.buildUpdateQuery(this.config);
  }
  toSQL() {
    const { typings: _typings, ...rest } = this.dialect.sqlToQuery(this.getSQL());
    return rest;
  }
  /** @internal */
  _prepare(isOneTimeQuery = true) {
    return this.session[isOneTimeQuery ? "prepareOneTimeQuery" : "prepareQuery"](
      this.dialect.sqlToQuery(this.getSQL()),
      this.config.returning,
      this.config.returning ? "all" : "run",
      true,
      void 0,
      {
        type: "insert",
        tables: extractUsedTable(this.config.table)
      }
    );
  }
  prepare() {
    return this._prepare(false);
  }
  run = (placeholderValues) => {
    return this._prepare().run(placeholderValues);
  };
  all = (placeholderValues) => {
    return this._prepare().all(placeholderValues);
  };
  get = (placeholderValues) => {
    return this._prepare().get(placeholderValues);
  };
  values = (placeholderValues) => {
    return this._prepare().values(placeholderValues);
  };
  async execute() {
    return this.config.returning ? this.all() : this.run();
  }
  $dynamic() {
    return this;
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/count.js
var SQLiteCountBuilder = class _SQLiteCountBuilder extends SQL {
  constructor(params) {
    super(_SQLiteCountBuilder.buildEmbeddedCount(params.source, params.filters).queryChunks);
    this.params = params;
    this.session = params.session;
    this.sql = _SQLiteCountBuilder.buildCount(
      params.source,
      params.filters
    );
  }
  sql;
  static [entityKind] = "SQLiteCountBuilderAsync";
  [Symbol.toStringTag] = "SQLiteCountBuilderAsync";
  session;
  static buildEmbeddedCount(source, filters) {
    return sql`(select count(*) from ${source}${sql.raw(" where ").if(filters)}${filters})`;
  }
  static buildCount(source, filters) {
    return sql`select count(*) from ${source}${sql.raw(" where ").if(filters)}${filters}`;
  }
  then(onfulfilled, onrejected) {
    return Promise.resolve(this.session.count(this.sql)).then(
      onfulfilled,
      onrejected
    );
  }
  catch(onRejected) {
    return this.then(void 0, onRejected);
  }
  finally(onFinally) {
    return this.then(
      (value) => {
        onFinally?.();
        return value;
      },
      (reason) => {
        onFinally?.();
        throw reason;
      }
    );
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/query.js
var RelationalQueryBuilder = class {
  constructor(mode, fullSchema, schema, tableNamesMap, table2, tableConfig, dialect, session) {
    this.mode = mode;
    this.fullSchema = fullSchema;
    this.schema = schema;
    this.tableNamesMap = tableNamesMap;
    this.table = table2;
    this.tableConfig = tableConfig;
    this.dialect = dialect;
    this.session = session;
  }
  static [entityKind] = "SQLiteAsyncRelationalQueryBuilder";
  findMany(config) {
    return this.mode === "sync" ? new SQLiteSyncRelationalQuery(
      this.fullSchema,
      this.schema,
      this.tableNamesMap,
      this.table,
      this.tableConfig,
      this.dialect,
      this.session,
      config ? config : {},
      "many"
    ) : new SQLiteRelationalQuery(
      this.fullSchema,
      this.schema,
      this.tableNamesMap,
      this.table,
      this.tableConfig,
      this.dialect,
      this.session,
      config ? config : {},
      "many"
    );
  }
  findFirst(config) {
    return this.mode === "sync" ? new SQLiteSyncRelationalQuery(
      this.fullSchema,
      this.schema,
      this.tableNamesMap,
      this.table,
      this.tableConfig,
      this.dialect,
      this.session,
      config ? { ...config, limit: 1 } : { limit: 1 },
      "first"
    ) : new SQLiteRelationalQuery(
      this.fullSchema,
      this.schema,
      this.tableNamesMap,
      this.table,
      this.tableConfig,
      this.dialect,
      this.session,
      config ? { ...config, limit: 1 } : { limit: 1 },
      "first"
    );
  }
};
var SQLiteRelationalQuery = class extends QueryPromise {
  constructor(fullSchema, schema, tableNamesMap, table2, tableConfig, dialect, session, config, mode) {
    super();
    this.fullSchema = fullSchema;
    this.schema = schema;
    this.tableNamesMap = tableNamesMap;
    this.table = table2;
    this.tableConfig = tableConfig;
    this.dialect = dialect;
    this.session = session;
    this.config = config;
    this.mode = mode;
  }
  static [entityKind] = "SQLiteAsyncRelationalQuery";
  /** @internal */
  mode;
  /** @internal */
  getSQL() {
    return this.dialect.buildRelationalQuery({
      fullSchema: this.fullSchema,
      schema: this.schema,
      tableNamesMap: this.tableNamesMap,
      table: this.table,
      tableConfig: this.tableConfig,
      queryConfig: this.config,
      tableAlias: this.tableConfig.tsName
    }).sql;
  }
  /** @internal */
  _prepare(isOneTimeQuery = false) {
    const { query, builtQuery } = this._toSQL();
    return this.session[isOneTimeQuery ? "prepareOneTimeQuery" : "prepareQuery"](
      builtQuery,
      void 0,
      this.mode === "first" ? "get" : "all",
      true,
      (rawRows, mapColumnValue) => {
        const rows = rawRows.map(
          (row) => mapRelationalRow(this.schema, this.tableConfig, row, query.selection, mapColumnValue)
        );
        if (this.mode === "first") {
          return rows[0];
        }
        return rows;
      }
    );
  }
  prepare() {
    return this._prepare(false);
  }
  _toSQL() {
    const query = this.dialect.buildRelationalQuery({
      fullSchema: this.fullSchema,
      schema: this.schema,
      tableNamesMap: this.tableNamesMap,
      table: this.table,
      tableConfig: this.tableConfig,
      queryConfig: this.config,
      tableAlias: this.tableConfig.tsName
    });
    const builtQuery = this.dialect.sqlToQuery(query.sql);
    return { query, builtQuery };
  }
  toSQL() {
    return this._toSQL().builtQuery;
  }
  /** @internal */
  executeRaw() {
    if (this.mode === "first") {
      return this._prepare(false).get();
    }
    return this._prepare(false).all();
  }
  async execute() {
    return this.executeRaw();
  }
};
var SQLiteSyncRelationalQuery = class extends SQLiteRelationalQuery {
  static [entityKind] = "SQLiteSyncRelationalQuery";
  sync() {
    return this.executeRaw();
  }
};

// node_modules/drizzle-orm/sqlite-core/query-builders/raw.js
var SQLiteRaw = class extends QueryPromise {
  constructor(execute, getSQL, action, dialect, mapBatchResult) {
    super();
    this.execute = execute;
    this.getSQL = getSQL;
    this.dialect = dialect;
    this.mapBatchResult = mapBatchResult;
    this.config = { action };
  }
  static [entityKind] = "SQLiteRaw";
  /** @internal */
  config;
  getQuery() {
    return { ...this.dialect.sqlToQuery(this.getSQL()), method: this.config.action };
  }
  mapResult(result, isFromBatch) {
    return isFromBatch ? this.mapBatchResult(result) : result;
  }
  _prepare() {
    return this;
  }
  /** @internal */
  isResponseInArrayMode() {
    return false;
  }
};

// node_modules/drizzle-orm/sqlite-core/db.js
var BaseSQLiteDatabase = class {
  constructor(resultKind, dialect, session, schema) {
    this.resultKind = resultKind;
    this.dialect = dialect;
    this.session = session;
    this._ = schema ? {
      schema: schema.schema,
      fullSchema: schema.fullSchema,
      tableNamesMap: schema.tableNamesMap
    } : {
      schema: void 0,
      fullSchema: {},
      tableNamesMap: {}
    };
    this.query = {};
    const query = this.query;
    if (this._.schema) {
      for (const [tableName, columns] of Object.entries(this._.schema)) {
        query[tableName] = new RelationalQueryBuilder(
          resultKind,
          schema.fullSchema,
          this._.schema,
          this._.tableNamesMap,
          schema.fullSchema[tableName],
          columns,
          dialect,
          session
        );
      }
    }
    this.$cache = { invalidate: async (_params) => {
    } };
  }
  static [entityKind] = "BaseSQLiteDatabase";
  query;
  /**
   * Creates a subquery that defines a temporary named result set as a CTE.
   *
   * It is useful for breaking down complex queries into simpler parts and for reusing the result set in subsequent parts of the query.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#with-clause}
   *
   * @param alias The alias for the subquery.
   *
   * Failure to provide an alias will result in a DrizzleTypeError, preventing the subquery from being referenced in other queries.
   *
   * @example
   *
   * ```ts
   * // Create a subquery with alias 'sq' and use it in the select query
   * const sq = db.$with('sq').as(db.select().from(users).where(eq(users.id, 42)));
   *
   * const result = await db.with(sq).select().from(sq);
   * ```
   *
   * To select arbitrary SQL values as fields in a CTE and reference them in other CTEs or in the main query, you need to add aliases to them:
   *
   * ```ts
   * // Select an arbitrary SQL value as a field in a CTE and reference it in the main query
   * const sq = db.$with('sq').as(db.select({
   *   name: sql<string>`upper(${users.name})`.as('name'),
   * })
   * .from(users));
   *
   * const result = await db.with(sq).select({ name: sq.name }).from(sq);
   * ```
   */
  $with = (alias, selection) => {
    const self = this;
    const as = (qb) => {
      if (typeof qb === "function") {
        qb = qb(new QueryBuilder(self.dialect));
      }
      return new Proxy(
        new WithSubquery(
          qb.getSQL(),
          selection ?? ("getSelectedFields" in qb ? qb.getSelectedFields() ?? {} : {}),
          alias,
          true
        ),
        new SelectionProxyHandler({ alias, sqlAliasedBehavior: "alias", sqlBehavior: "error" })
      );
    };
    return { as };
  };
  $count(source, filters) {
    return new SQLiteCountBuilder({ source, filters, session: this.session });
  }
  /**
   * Incorporates a previously defined CTE (using `$with`) into the main query.
   *
   * This method allows the main query to reference a temporary named result set.
   *
   * See docs: {@link https://orm.drizzle.team/docs/select#with-clause}
   *
   * @param queries The CTEs to incorporate into the main query.
   *
   * @example
   *
   * ```ts
   * // Define a subquery 'sq' as a CTE using $with
   * const sq = db.$with('sq').as(db.select().from(users).where(eq(users.id, 42)));
   *
   * // Incorporate the CTE 'sq' into the main query and select from it
   * const result = await db.with(sq).select().from(sq);
   * ```
   */
  with(...queries) {
    const self = this;
    function select(fields) {
      return new SQLiteSelectBuilder({
        fields: fields ?? void 0,
        session: self.session,
        dialect: self.dialect,
        withList: queries
      });
    }
    function selectDistinct(fields) {
      return new SQLiteSelectBuilder({
        fields: fields ?? void 0,
        session: self.session,
        dialect: self.dialect,
        withList: queries,
        distinct: true
      });
    }
    function update(table2) {
      return new SQLiteUpdateBuilder(table2, self.session, self.dialect, queries);
    }
    function insert(into) {
      return new SQLiteInsertBuilder(into, self.session, self.dialect, queries);
    }
    function delete_(from) {
      return new SQLiteDeleteBase(from, self.session, self.dialect, queries);
    }
    return { select, selectDistinct, update, insert, delete: delete_ };
  }
  select(fields) {
    return new SQLiteSelectBuilder({ fields: fields ?? void 0, session: this.session, dialect: this.dialect });
  }
  selectDistinct(fields) {
    return new SQLiteSelectBuilder({
      fields: fields ?? void 0,
      session: this.session,
      dialect: this.dialect,
      distinct: true
    });
  }
  /**
   * Creates an update query.
   *
   * Calling this method without `.where()` clause will update all rows in a table. The `.where()` clause specifies which rows should be updated.
   *
   * Use `.set()` method to specify which values to update.
   *
   * See docs: {@link https://orm.drizzle.team/docs/update}
   *
   * @param table The table to update.
   *
   * @example
   *
   * ```ts
   * // Update all rows in the 'cars' table
   * await db.update(cars).set({ color: 'red' });
   *
   * // Update rows with filters and conditions
   * await db.update(cars).set({ color: 'red' }).where(eq(cars.brand, 'BMW'));
   *
   * // Update with returning clause
   * const updatedCar: Car[] = await db.update(cars)
   *   .set({ color: 'red' })
   *   .where(eq(cars.id, 1))
   *   .returning();
   * ```
   */
  update(table2) {
    return new SQLiteUpdateBuilder(table2, this.session, this.dialect);
  }
  $cache;
  /**
   * Creates an insert query.
   *
   * Calling this method will create new rows in a table. Use `.values()` method to specify which values to insert.
   *
   * See docs: {@link https://orm.drizzle.team/docs/insert}
   *
   * @param table The table to insert into.
   *
   * @example
   *
   * ```ts
   * // Insert one row
   * await db.insert(cars).values({ brand: 'BMW' });
   *
   * // Insert multiple rows
   * await db.insert(cars).values([{ brand: 'BMW' }, { brand: 'Porsche' }]);
   *
   * // Insert with returning clause
   * const insertedCar: Car[] = await db.insert(cars)
   *   .values({ brand: 'BMW' })
   *   .returning();
   * ```
   */
  insert(into) {
    return new SQLiteInsertBuilder(into, this.session, this.dialect);
  }
  /**
   * Creates a delete query.
   *
   * Calling this method without `.where()` clause will delete all rows in a table. The `.where()` clause specifies which rows should be deleted.
   *
   * See docs: {@link https://orm.drizzle.team/docs/delete}
   *
   * @param table The table to delete from.
   *
   * @example
   *
   * ```ts
   * // Delete all rows in the 'cars' table
   * await db.delete(cars);
   *
   * // Delete rows with filters and conditions
   * await db.delete(cars).where(eq(cars.color, 'green'));
   *
   * // Delete with returning clause
   * const deletedCar: Car[] = await db.delete(cars)
   *   .where(eq(cars.id, 1))
   *   .returning();
   * ```
   */
  delete(from) {
    return new SQLiteDeleteBase(from, this.session, this.dialect);
  }
  run(query) {
    const sequel = typeof query === "string" ? sql.raw(query) : query.getSQL();
    if (this.resultKind === "async") {
      return new SQLiteRaw(
        async () => this.session.run(sequel),
        () => sequel,
        "run",
        this.dialect,
        this.session.extractRawRunValueFromBatchResult.bind(this.session)
      );
    }
    return this.session.run(sequel);
  }
  all(query) {
    const sequel = typeof query === "string" ? sql.raw(query) : query.getSQL();
    if (this.resultKind === "async") {
      return new SQLiteRaw(
        async () => this.session.all(sequel),
        () => sequel,
        "all",
        this.dialect,
        this.session.extractRawAllValueFromBatchResult.bind(this.session)
      );
    }
    return this.session.all(sequel);
  }
  get(query) {
    const sequel = typeof query === "string" ? sql.raw(query) : query.getSQL();
    if (this.resultKind === "async") {
      return new SQLiteRaw(
        async () => this.session.get(sequel),
        () => sequel,
        "get",
        this.dialect,
        this.session.extractRawGetValueFromBatchResult.bind(this.session)
      );
    }
    return this.session.get(sequel);
  }
  values(query) {
    const sequel = typeof query === "string" ? sql.raw(query) : query.getSQL();
    if (this.resultKind === "async") {
      return new SQLiteRaw(
        async () => this.session.values(sequel),
        () => sequel,
        "values",
        this.dialect,
        this.session.extractRawValuesValueFromBatchResult.bind(this.session)
      );
    }
    return this.session.values(sequel);
  }
  transaction(transaction, config) {
    return this.session.transaction(transaction, config);
  }
};

// node_modules/drizzle-orm/cache/core/cache.js
var Cache = class {
  static [entityKind] = "Cache";
};
var NoopCache = class extends Cache {
  strategy() {
    return "all";
  }
  static [entityKind] = "NoopCache";
  async get(_key) {
    return void 0;
  }
  async put(_hashedQuery, _response, _tables, _config) {
  }
  async onMutate(_params) {
  }
};
async function hashQuery(sql2, params) {
  const dataToHash = `${sql2}-${JSON.stringify(params)}`;
  const encoder = new TextEncoder();
  const data = encoder.encode(dataToHash);
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  const hashArray = [...new Uint8Array(hashBuffer)];
  const hashHex = hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
  return hashHex;
}

// node_modules/drizzle-orm/sqlite-core/session.js
var ExecuteResultSync = class extends QueryPromise {
  constructor(resultCb) {
    super();
    this.resultCb = resultCb;
  }
  static [entityKind] = "ExecuteResultSync";
  async execute() {
    return this.resultCb();
  }
  sync() {
    return this.resultCb();
  }
};
var SQLitePreparedQuery = class {
  constructor(mode, executeMethod, query, cache, queryMetadata, cacheConfig) {
    this.mode = mode;
    this.executeMethod = executeMethod;
    this.query = query;
    this.cache = cache;
    this.queryMetadata = queryMetadata;
    this.cacheConfig = cacheConfig;
    if (cache && cache.strategy() === "all" && cacheConfig === void 0) {
      this.cacheConfig = { enable: true, autoInvalidate: true };
    }
    if (!this.cacheConfig?.enable) {
      this.cacheConfig = void 0;
    }
  }
  static [entityKind] = "PreparedQuery";
  /** @internal */
  joinsNotNullableMap;
  /** @internal */
  async queryWithCache(queryString, params, query) {
    if (this.cache === void 0 || is(this.cache, NoopCache) || this.queryMetadata === void 0) {
      try {
        return await query();
      } catch (e) {
        throw new DrizzleQueryError(queryString, params, e);
      }
    }
    if (this.cacheConfig && !this.cacheConfig.enable) {
      try {
        return await query();
      } catch (e) {
        throw new DrizzleQueryError(queryString, params, e);
      }
    }
    if ((this.queryMetadata.type === "insert" || this.queryMetadata.type === "update" || this.queryMetadata.type === "delete") && this.queryMetadata.tables.length > 0) {
      try {
        const [res] = await Promise.all([
          query(),
          this.cache.onMutate({ tables: this.queryMetadata.tables })
        ]);
        return res;
      } catch (e) {
        throw new DrizzleQueryError(queryString, params, e);
      }
    }
    if (!this.cacheConfig) {
      try {
        return await query();
      } catch (e) {
        throw new DrizzleQueryError(queryString, params, e);
      }
    }
    if (this.queryMetadata.type === "select") {
      const fromCache = await this.cache.get(
        this.cacheConfig.tag ?? await hashQuery(queryString, params),
        this.queryMetadata.tables,
        this.cacheConfig.tag !== void 0,
        this.cacheConfig.autoInvalidate
      );
      if (fromCache === void 0) {
        let result;
        try {
          result = await query();
        } catch (e) {
          throw new DrizzleQueryError(queryString, params, e);
        }
        await this.cache.put(
          this.cacheConfig.tag ?? await hashQuery(queryString, params),
          result,
          // make sure we send tables that were used in a query only if user wants to invalidate it on each write
          this.cacheConfig.autoInvalidate ? this.queryMetadata.tables : [],
          this.cacheConfig.tag !== void 0,
          this.cacheConfig.config
        );
        return result;
      }
      return fromCache;
    }
    try {
      return await query();
    } catch (e) {
      throw new DrizzleQueryError(queryString, params, e);
    }
  }
  getQuery() {
    return this.query;
  }
  mapRunResult(result, _isFromBatch) {
    return result;
  }
  mapAllResult(_result, _isFromBatch) {
    throw new Error("Not implemented");
  }
  mapGetResult(_result, _isFromBatch) {
    throw new Error("Not implemented");
  }
  execute(placeholderValues) {
    if (this.mode === "async") {
      return this[this.executeMethod](placeholderValues);
    }
    return new ExecuteResultSync(() => this[this.executeMethod](placeholderValues));
  }
  mapResult(response, isFromBatch) {
    switch (this.executeMethod) {
      case "run": {
        return this.mapRunResult(response, isFromBatch);
      }
      case "all": {
        return this.mapAllResult(response, isFromBatch);
      }
      case "get": {
        return this.mapGetResult(response, isFromBatch);
      }
    }
  }
};
var SQLiteSession = class {
  constructor(dialect) {
    this.dialect = dialect;
  }
  static [entityKind] = "SQLiteSession";
  prepareOneTimeQuery(query, fields, executeMethod, isResponseInArrayMode, customResultMapper, queryMetadata, cacheConfig) {
    return this.prepareQuery(
      query,
      fields,
      executeMethod,
      isResponseInArrayMode,
      customResultMapper,
      queryMetadata,
      cacheConfig
    );
  }
  run(query) {
    const staticQuery = this.dialect.sqlToQuery(query);
    try {
      return this.prepareOneTimeQuery(staticQuery, void 0, "run", false).run();
    } catch (err) {
      throw new DrizzleError({ cause: err, message: `Failed to run the query '${staticQuery.sql}'` });
    }
  }
  /** @internal */
  extractRawRunValueFromBatchResult(result) {
    return result;
  }
  all(query) {
    return this.prepareOneTimeQuery(this.dialect.sqlToQuery(query), void 0, "run", false).all();
  }
  /** @internal */
  extractRawAllValueFromBatchResult(_result) {
    throw new Error("Not implemented");
  }
  get(query) {
    return this.prepareOneTimeQuery(this.dialect.sqlToQuery(query), void 0, "run", false).get();
  }
  /** @internal */
  extractRawGetValueFromBatchResult(_result) {
    throw new Error("Not implemented");
  }
  values(query) {
    return this.prepareOneTimeQuery(this.dialect.sqlToQuery(query), void 0, "run", false).values();
  }
  async count(sql2) {
    const result = await this.values(sql2);
    return result[0][0];
  }
  /** @internal */
  extractRawValuesValueFromBatchResult(_result) {
    throw new Error("Not implemented");
  }
};
var SQLiteTransaction = class extends BaseSQLiteDatabase {
  constructor(resultType, dialect, session, schema, nestedIndex = 0) {
    super(resultType, dialect, session, schema);
    this.schema = schema;
    this.nestedIndex = nestedIndex;
  }
  static [entityKind] = "SQLiteTransaction";
  rollback() {
    throw new TransactionRollbackError();
  }
};

// node_modules/drizzle-orm/sqlite-proxy/session.js
var SQLiteRemoteSession = class extends SQLiteSession {
  constructor(client, dialect, schema, batchCLient, options = {}) {
    super(dialect);
    this.client = client;
    this.schema = schema;
    this.batchCLient = batchCLient;
    this.logger = options.logger ?? new NoopLogger();
    this.cache = options.cache ?? new NoopCache();
  }
  static [entityKind] = "SQLiteRemoteSession";
  logger;
  cache;
  prepareQuery(query, fields, executeMethod, isResponseInArrayMode, customResultMapper, queryMetadata, cacheConfig) {
    return new RemotePreparedQuery(
      this.client,
      query,
      this.logger,
      this.cache,
      queryMetadata,
      cacheConfig,
      fields,
      executeMethod,
      isResponseInArrayMode,
      customResultMapper
    );
  }
  async batch(queries) {
    const preparedQueries = [];
    const builtQueries = [];
    for (const query of queries) {
      const preparedQuery = query._prepare();
      const builtQuery = preparedQuery.getQuery();
      preparedQueries.push(preparedQuery);
      builtQueries.push({ sql: builtQuery.sql, params: builtQuery.params, method: builtQuery.method });
    }
    const batchResults = await this.batchCLient(builtQueries);
    return batchResults.map((result, i) => preparedQueries[i].mapResult(result, true));
  }
  async transaction(transaction, config) {
    const tx = new SQLiteProxyTransaction("async", this.dialect, this, this.schema);
    await this.run(sql.raw(`begin${config?.behavior ? " " + config.behavior : ""}`));
    try {
      const result = await transaction(tx);
      await this.run(sql`commit`);
      return result;
    } catch (err) {
      await this.run(sql`rollback`);
      throw err;
    }
  }
  extractRawAllValueFromBatchResult(result) {
    return result.rows;
  }
  extractRawGetValueFromBatchResult(result) {
    return result.rows[0];
  }
  extractRawValuesValueFromBatchResult(result) {
    return result.rows;
  }
};
var SQLiteProxyTransaction = class _SQLiteProxyTransaction extends SQLiteTransaction {
  static [entityKind] = "SQLiteProxyTransaction";
  async transaction(transaction) {
    const savepointName = `sp${this.nestedIndex}`;
    const tx = new _SQLiteProxyTransaction("async", this.dialect, this.session, this.schema, this.nestedIndex + 1);
    await this.session.run(sql.raw(`savepoint ${savepointName}`));
    try {
      const result = await transaction(tx);
      await this.session.run(sql.raw(`release savepoint ${savepointName}`));
      return result;
    } catch (err) {
      await this.session.run(sql.raw(`rollback to savepoint ${savepointName}`));
      throw err;
    }
  }
};
var RemotePreparedQuery = class extends SQLitePreparedQuery {
  constructor(client, query, logger, cache, queryMetadata, cacheConfig, fields, executeMethod, _isResponseInArrayMode, customResultMapper) {
    super("async", executeMethod, query, cache, queryMetadata, cacheConfig);
    this.client = client;
    this.logger = logger;
    this.fields = fields;
    this._isResponseInArrayMode = _isResponseInArrayMode;
    this.customResultMapper = customResultMapper;
    this.customResultMapper = customResultMapper;
    this.method = executeMethod;
  }
  static [entityKind] = "SQLiteProxyPreparedQuery";
  method;
  getQuery() {
    return { ...this.query, method: this.method };
  }
  async run(placeholderValues) {
    const params = fillPlaceholders(this.query.params, placeholderValues ?? {});
    this.logger.logQuery(this.query.sql, params);
    return await this.queryWithCache(this.query.sql, params, async () => {
      return await this.client(this.query.sql, params, "run");
    });
  }
  mapAllResult(rows, isFromBatch) {
    if (isFromBatch) {
      rows = rows.rows;
    }
    if (!this.fields && !this.customResultMapper) {
      return rows;
    }
    if (this.customResultMapper) {
      return this.customResultMapper(rows);
    }
    return rows.map((row) => {
      return mapResultRow(
        this.fields,
        row,
        this.joinsNotNullableMap
      );
    });
  }
  async all(placeholderValues) {
    const { query, logger, client } = this;
    const params = fillPlaceholders(query.params, placeholderValues ?? {});
    logger.logQuery(query.sql, params);
    const { rows } = await this.queryWithCache(query.sql, params, async () => {
      return await client(query.sql, params, "all");
    });
    return this.mapAllResult(rows);
  }
  async get(placeholderValues) {
    const { query, logger, client } = this;
    const params = fillPlaceholders(query.params, placeholderValues ?? {});
    logger.logQuery(query.sql, params);
    const clientResult = await this.queryWithCache(query.sql, params, async () => {
      return await client(query.sql, params, "get");
    });
    return this.mapGetResult(clientResult.rows);
  }
  mapGetResult(rows, isFromBatch) {
    if (isFromBatch) {
      rows = rows.rows;
    }
    const row = rows;
    if (!this.fields && !this.customResultMapper) {
      return row;
    }
    if (!row) {
      return void 0;
    }
    if (this.customResultMapper) {
      return this.customResultMapper([rows]);
    }
    return mapResultRow(
      this.fields,
      row,
      this.joinsNotNullableMap
    );
  }
  async values(placeholderValues) {
    const params = fillPlaceholders(this.query.params, placeholderValues ?? {});
    this.logger.logQuery(this.query.sql, params);
    const clientResult = await this.queryWithCache(this.query.sql, params, async () => {
      return await this.client(this.query.sql, params, "values");
    });
    return clientResult.rows;
  }
  /** @internal */
  isResponseInArrayMode() {
    return this._isResponseInArrayMode;
  }
};

// node_modules/drizzle-orm/sqlite-proxy/driver.js
var SqliteRemoteDatabase = class extends BaseSQLiteDatabase {
  static [entityKind] = "SqliteRemoteDatabase";
  async batch(batch) {
    return this.session.batch(batch);
  }
};
function drizzle(callback, batchCallback, config) {
  const dialect = new SQLiteAsyncDialect({ casing: config?.casing });
  let logger;
  let cache;
  let _batchCallback;
  let _config = {};
  if (batchCallback) {
    if (typeof batchCallback === "function") {
      _batchCallback = batchCallback;
      _config = config ?? {};
    } else {
      _batchCallback = void 0;
      _config = batchCallback;
    }
    if (_config.logger === true) {
      logger = new DefaultLogger();
    } else if (_config.logger !== false) {
      logger = _config.logger;
      cache = _config.cache;
    }
  }
  let schema;
  if (_config.schema) {
    const tablesConfig = extractTablesRelationalConfig(
      _config.schema,
      createTableRelationsHelpers
    );
    schema = {
      fullSchema: _config.schema,
      schema: tablesConfig.tables,
      tableNamesMap: tablesConfig.tableNamesMap
    };
  }
  const session = new SQLiteRemoteSession(callback, dialect, schema, _batchCallback, { logger, cache });
  const db = new SqliteRemoteDatabase("async", dialect, session, schema);
  db.$cache = cache;
  if (db.$cache) {
    db.$cache["invalidate"] = cache?.onMutate;
  }
  return db;
}

// node_modules/@haex-space/vault-sdk/dist/index.mjs
var __defProp = Object.defineProperty;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var init_esm_shims = __esm({
  "node_modules/.pnpm/tsup@8.5.1_jiti@2.7.0_postcss@8.5.29_typescript@6.0.3_yaml@2.9.0/node_modules/tsup/assets/esm_shims.js"() {
  }
});
async function deriveKeyFromPassword(password, salt) {
  const encoder = new TextEncoder();
  const passwordBuffer = encoder.encode(password);
  const saltBuffer = new Uint8Array(salt);
  const keyMaterial = await crypto.subtle.importKey(
    "raw",
    passwordBuffer,
    "PBKDF2",
    false,
    ["deriveKey"]
  );
  return await crypto.subtle.deriveKey(
    {
      name: "PBKDF2",
      salt: saltBuffer,
      iterations: PBKDF2_ITERATIONS,
      hash: "SHA-256"
    },
    keyMaterial,
    { name: ALGORITHM, length: KEY_LENGTH },
    false,
    // not extractable
    ["encrypt", "decrypt"]
  );
}
function arrayBufferToBase64(buffer) {
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  if (typeof Buffer !== "undefined") {
    return Buffer.from(bytes).toString("base64");
  }
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    const byte = bytes[i];
    if (byte !== void 0) {
      binary += String.fromCharCode(byte);
    }
  }
  return btoa(binary);
}
function base64ToArrayBuffer(base64) {
  if (typeof Buffer !== "undefined") {
    return new Uint8Array(Buffer.from(base64, "base64"));
  }
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
var PBKDF2_ITERATIONS;
var KEY_LENGTH;
var ALGORITHM;
var init_vaultKey = __esm({
  "src/crypto/vaultKey.ts"() {
    init_esm_shims();
    PBKDF2_ITERATIONS = 6e5;
    KEY_LENGTH = 256;
    ALGORITHM = "AES-GCM";
  }
});
var userKeypair_exports = {};
__export(userKeypair_exports, {
  KEY_AGREEMENT_ALGO: () => KEY_AGREEMENT_ALGO,
  SIGNING_ALGO: () => SIGNING_ALGO,
  decryptPrivateKeyAsync: () => decryptPrivateKeyAsync,
  encryptPrivateKeyAsync: () => encryptPrivateKeyAsync,
  exportUserKeypairAsync: () => exportUserKeypairAsync,
  generateUserKeypairAsync: () => generateUserKeypairAsync,
  importPrivateKeyForKeyAgreementAsync: () => importPrivateKeyForKeyAgreementAsync,
  importPublicKeyForKeyAgreementAsync: () => importPublicKeyForKeyAgreementAsync,
  importUserPrivateKeyAsync: () => importUserPrivateKeyAsync,
  importUserPublicKeyAsync: () => importUserPublicKeyAsync
});
async function generateUserKeypairAsync() {
  const signing = await crypto.subtle.generateKey(SIGNING_ALGO, true, ["sign", "verify"]);
  const agreement = await crypto.subtle.generateKey(KEY_AGREEMENT_ALGO, true, ["deriveBits"]);
  return {
    signingPublicKey: signing.publicKey,
    signingPrivateKey: signing.privateKey,
    agreementPublicKey: agreement.publicKey,
    agreementPrivateKey: agreement.privateKey
  };
}
async function exportUserKeypairAsync(keypair) {
  const [sigPub, sigPriv, agrPub, agrPriv] = await Promise.all([
    crypto.subtle.exportKey("spki", keypair.signingPublicKey),
    crypto.subtle.exportKey("pkcs8", keypair.signingPrivateKey),
    crypto.subtle.exportKey("spki", keypair.agreementPublicKey),
    crypto.subtle.exportKey("pkcs8", keypair.agreementPrivateKey)
  ]);
  return {
    signingPublicKey: arrayBufferToBase64(sigPub),
    signingPrivateKey: arrayBufferToBase64(sigPriv),
    agreementPublicKey: arrayBufferToBase64(agrPub),
    agreementPrivateKey: arrayBufferToBase64(agrPriv)
  };
}
async function importUserPublicKeyAsync(base64) {
  return crypto.subtle.importKey("spki", base64ToArrayBuffer(base64), SIGNING_ALGO, true, ["verify"]);
}
async function importUserPrivateKeyAsync(base64) {
  return crypto.subtle.importKey("pkcs8", base64ToArrayBuffer(base64), SIGNING_ALGO, true, ["sign"]);
}
async function importPublicKeyForKeyAgreementAsync(base64) {
  return crypto.subtle.importKey("spki", base64ToArrayBuffer(base64), KEY_AGREEMENT_ALGO, true, []);
}
async function importPrivateKeyForKeyAgreementAsync(base64) {
  return crypto.subtle.importKey("pkcs8", base64ToArrayBuffer(base64), KEY_AGREEMENT_ALGO, true, ["deriveBits"]);
}
async function encryptPrivateKeyAsync(privateKeyBase64, password) {
  const salt = crypto.getRandomValues(new Uint8Array(32));
  const derivedKey = await deriveKeyFromPassword(password, salt);
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: nonce },
    derivedKey,
    new TextEncoder().encode(privateKeyBase64)
  );
  return {
    encryptedPrivateKey: arrayBufferToBase64(encrypted),
    nonce: arrayBufferToBase64(nonce),
    salt: arrayBufferToBase64(salt)
  };
}
async function decryptPrivateKeyAsync(encryptedPrivateKey, nonce, salt, password) {
  const derivedKey = await deriveKeyFromPassword(password, base64ToArrayBuffer(salt));
  const decrypted = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: base64ToArrayBuffer(nonce) },
    derivedKey,
    base64ToArrayBuffer(encryptedPrivateKey)
  );
  return new TextDecoder().decode(decrypted);
}
var SIGNING_ALGO;
var KEY_AGREEMENT_ALGO;
var init_userKeypair = __esm({
  "src/crypto/userKeypair.ts"() {
    init_esm_shims();
    init_vaultKey();
    SIGNING_ALGO = { name: "Ed25519" };
    KEY_AGREEMENT_ALGO = { name: "X25519" };
  }
});
init_esm_shims();
init_esm_shims();
init_esm_shims();
function installLocalStoragePolyfill() {
  if (typeof window === "undefined") {
    return;
  }
  console.log("[HaexSpace] Storage Polyfill loading immediately");
  let localStorageWorks = false;
  try {
    const testKey = "__ls_test__";
    localStorage.setItem(testKey, testKey);
    localStorage.removeItem(testKey);
    localStorageWorks = true;
  } catch {
    console.warn("[HaexSpace] localStorage blocked \u2013 using in-memory fallback");
  }
  if (!localStorageWorks) {
    const lsStorage = /* @__PURE__ */ new Map();
    const localStoragePoly = {
      getItem(key) {
        return lsStorage.get(key) || null;
      },
      setItem(key, value) {
        lsStorage.set(key, String(value));
      },
      removeItem(key) {
        lsStorage.delete(key);
      },
      clear() {
        lsStorage.clear();
      },
      get length() {
        return lsStorage.size;
      },
      key(index) {
        return Array.from(lsStorage.keys())[index] || null;
      }
    };
    try {
      Object.defineProperty(window, "localStorage", {
        value: localStoragePoly,
        writable: true,
        configurable: true
      });
    } catch {
      window.localStorage = localStoragePoly;
    }
    console.log("[HaexSpace] localStorage replaced with in-memory polyfill");
  }
}
function installSessionStoragePolyfill() {
  if (typeof window === "undefined") {
    return;
  }
  try {
    const sessionStoragePoly = {
      getItem() {
        return null;
      },
      setItem() {
      },
      removeItem() {
      },
      clear() {
      },
      get length() {
        return 0;
      },
      key() {
        return null;
      }
    };
    Object.defineProperty(window, "sessionStorage", {
      value: sessionStoragePoly,
      writable: true,
      configurable: true
    });
  } catch {
    window.sessionStorage = {
      getItem: () => null,
      setItem: () => {
      },
      removeItem: () => {
      },
      clear: () => {
      },
      get length() {
        return 0;
      },
      key: () => null
    };
  }
  console.log("[HaexSpace] sessionStorage polyfill installed");
}
init_esm_shims();
function installCookiePolyfill() {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return;
  }
  let cookiesWork = false;
  try {
    document.cookie = "__cookie_test__=1";
    cookiesWork = document.cookie.indexOf("__cookie_test__") !== -1;
  } catch {
    console.warn("[HaexSpace] Cookies blocked \u2013 using in-memory fallback");
  }
  if (!cookiesWork) {
    const cookieStore = /* @__PURE__ */ new Map();
    Object.defineProperty(document, "cookie", {
      get() {
        const cookies = [];
        cookieStore.forEach((value, key) => {
          cookies.push(`${key}=${value}`);
        });
        return cookies.join("; ");
      },
      set(cookieString) {
        const parts = cookieString.split(";").map((p) => p.trim());
        const [keyValue] = parts;
        if (!keyValue) return;
        const [key, value] = keyValue.split("=");
        if (!key) return;
        const options = {};
        for (let i = 1; i < parts.length; i++) {
          const part = parts[i];
          if (!part) continue;
          const parts_split = part.split("=");
          const optKey = parts_split[0];
          const optValue = parts_split[1];
          if (optKey) {
            options[optKey.toLowerCase()] = optValue || true;
          }
        }
        const expiresValue = options.expires;
        if (expiresValue && typeof expiresValue === "string") {
          const expiresDate = new Date(expiresValue);
          if (expiresDate < /* @__PURE__ */ new Date()) {
            cookieStore.delete(key);
            return;
          }
        }
        const maxAgeValue = options["max-age"];
        if (typeof maxAgeValue === "string" && maxAgeValue === "0") {
          cookieStore.delete(key);
          return;
        }
        cookieStore.set(key, value || "");
      },
      configurable: true
    });
    console.log("[HaexSpace] Cookie polyfill installed");
  }
}
init_esm_shims();
function installHistoryPolyfill() {
  if (typeof window === "undefined" || typeof history === "undefined") {
    return;
  }
  const install = () => {
    console.log("[HaexSpace] History Patch loading");
    const originalPushState = history.pushState;
    const originalReplaceState = history.replaceState;
    let skipNextPush = false;
    let skipNextReplace = false;
    history.pushState = function(state, title, url) {
      console.log("[HaexSpace] pushState called:", url, "skip:", skipNextPush);
      if (skipNextPush) {
        skipNextPush = false;
        console.log("[HaexSpace] pushState skipped");
        return;
      }
      try {
        return originalPushState.call(this, state, title, url);
      } catch (e) {
        if (e.name === "SecurityError" && url) {
          const urlString = url.toString();
          let hashUrl = urlString.replace(/^\/#/, "");
          hashUrl = hashUrl.startsWith("#") ? hashUrl : "#" + hashUrl;
          console.log("[HaexSpace] SecurityError - setting hash to:", hashUrl);
          skipNextPush = true;
          window.location.hash = hashUrl.replace(/^#/, "");
          return;
        }
        throw e;
      }
    };
    history.replaceState = function(state, title, url) {
      console.log("[HaexSpace] replaceState called:", url, "skip:", skipNextReplace);
      if (skipNextReplace) {
        skipNextReplace = false;
        console.log("[HaexSpace] replaceState skipped");
        return;
      }
      try {
        return originalReplaceState.call(this, state, title, url);
      } catch (e) {
        if (e.name === "SecurityError" && url) {
          const urlString = url.toString();
          let hashUrl = urlString.replace(/^\/#/, "");
          hashUrl = hashUrl.startsWith("#") ? hashUrl : "#" + hashUrl;
          console.log("[HaexSpace] SecurityError - setting hash to:", hashUrl);
          skipNextReplace = true;
          window.location.hash = hashUrl.replace(/^#/, "");
          return;
        }
        throw e;
      }
    };
    console.log("[HaexSpace] History API patched");
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", install, { once: true });
  } else {
    install();
  }
}
init_esm_shims();
init_esm_shims();
var HAEXSPACE_MESSAGE_TYPES = {
  /** Debug message for development/troubleshooting */
  DEBUG: "haexspace:debug",
  /** Console forwarding from extension iframe */
  CONSOLE_FORWARD: "console.forward",
  /**
   * Sent from main window to iframe on the shared window listener, carrying
   * one `MessagePort` in `event.ports[0]`. Once received, the SDK switches
   * to port-based messaging and never reads the window listener again.
   *
   * Payload: `{ type: PORT_INIT }` — no data. The port itself is the payload.
   */
  PORT_INIT: "haexspace:port:init",
  /**
   * Sent from SDK to main window *over the MessagePort* after the port is
   * installed. Main uses this to mark the iframe as ready and flush any
   * events buffered during the handshake window. Only valid on the port —
   * a READY sent over window.postMessage is ignored.
   *
   * Payload: `{ type: PORT_READY }` — no data.
   */
  PORT_READY: "haexspace:port:ready",
  /**
   * One MCP message of the tool connection (holzi spec 018), in both directions over the
   * MessagePort: the host calls the extension's tools, `sdk.tools` answers.
   *
   * Payload: `{ type: MCP, message }`, where `message` is a JSON-RPC 2.0 object.
   */
  MCP: "haexspace:mcp"
};
function installDebugDiagnostics() {
  if (typeof window === "undefined") {
    return;
  }
  const hasParent = window.parent && window.parent !== window;
  console.log("[HaexSpace] hasParent:", hasParent);
  if (hasParent) {
    console.log("[HaexSpace] Attempting to send debug message to parent...");
    window.parent.postMessage({
      type: HAEXSPACE_MESSAGE_TYPES.DEBUG,
      data: `[Polyfills] window.parent test: exists=${!!window.parent}, different=${hasParent}, selfIsTop=${window.self === window.top}`
    }, "*");
    console.log("[HaexSpace] Debug message sent!");
  } else {
    console.log("[HaexSpace] No parent window or parent === window");
  }
}
init_esm_shims();
init_esm_shims();
var originalConsole = {
  log: console.log,
  info: console.info,
  warn: console.warn,
  error: console.error,
  debug: console.debug
};
function serializeArgs(args) {
  return args.map((arg) => {
    if (arg === null) return "null";
    if (arg === void 0) return "undefined";
    if (typeof arg === "object") {
      try {
        return JSON.stringify(arg, null, 2);
      } catch {
        return String(arg);
      }
    }
    return String(arg);
  }).join(" ");
}
function interceptConsole(level) {
  console[level] = function(...args) {
    originalConsole[level].apply(console, args);
    if (window.self !== window.top && window.parent) {
      try {
        const message = serializeArgs(args);
        const timestamp = (/* @__PURE__ */ new Date()).toLocaleTimeString();
        window.parent.postMessage(
          {
            type: HAEXSPACE_MESSAGE_TYPES.CONSOLE_FORWARD,
            data: {
              timestamp,
              level,
              message
            },
            timestamp: Date.now()
          },
          "*"
        );
      } catch (error) {
        originalConsole.error("[HaexSpace] Failed to forward console message:", error);
      }
    }
  };
}
function installConsoleForwarding(debug = false) {
  if (typeof window === "undefined") {
    return;
  }
  if (window.self === window.top) {
    return;
  }
  if (!debug) {
    console.log("[HaexSpace] Console forwarding disabled (not in debug mode)");
    return;
  }
  interceptConsole("log");
  interceptConsole("info");
  interceptConsole("warn");
  interceptConsole("error");
  interceptConsole("debug");
  console.log("[HaexSpace] Console forwarding installed");
}
function installPolyfills() {
  if (typeof window === "undefined") {
    return;
  }
  installLocalStoragePolyfill();
  installSessionStoragePolyfill();
  installCookiePolyfill();
  installHistoryPolyfill();
  installDebugDiagnostics();
}
if (typeof window !== "undefined") {
  installPolyfills();
}
init_esm_shims();
init_esm_shims();
init_esm_shims();
var HAEXTENSION_EVENTS = {
  /** Context (theme, locale, platform) has changed */
  CONTEXT_CHANGED: "haextension:context:changed",
  /** Search request from HaexHub */
  SEARCH_REQUEST: "haextension:search:request",
  /** File system change detected (from native file watcher) */
  FILE_CHANGED: "filesync:file-changed",
  /** Tables have been updated via sync (CRDT pull from server) */
  SYNC_TABLES_UPDATED: "haextension:sync:tables-updated",
  /** A runtime permission prompt was resolved (granted/denied) by the user.
   *  The SDK uses this to auto-retry the original request; extensions may also
   *  subscribe via `client.on(HAEXTENSION_EVENTS.PERMISSION_RESOLVED, ...)`. */
  PERMISSION_RESOLVED: "extension:permission-resolved"
};
var NOTIFICATION_EVENTS = {
  /** A click on one of this extension's notifications (body or action button). */
  CLICK: "haextension:notification:click"
};
var EXTERNAL_EVENTS = {
  /** External request from authorized client */
  REQUEST: "haextension:external:request",
  /** AI action request (tool calls from AI assistant) */
  ACTION_REQUEST: "haextension:action:request",
  /** New external client requesting authorization */
  AUTHORIZATION_REQUEST: "external:authorization-request"
};
var SHELL_EVENTS = {
  /** PTY output data from a shell session */
  OUTPUT: "shell:output",
  /** Shell session has exited */
  EXIT: "shell:exit"
};
var MAIL_EVENTS = {
  /** A watched mailbox's UID high-water mark advanced — new mail arrived. */
  NEW_MESSAGES: "mail:new-messages"
};
var DEFAULT_TIMEOUT = 3e4;
var TABLE_SEPARATOR = "__";
function getTableName2(publicKey, extensionName, tableName) {
  return `${publicKey}${TABLE_SEPARATOR}${extensionName}${TABLE_SEPARATOR}${tableName}`;
}
function isPermissionPromptError(error) {
  return typeof error === "object" && error !== null && "code" in error && error.code === 1004;
}
var HaexVaultSdkError = class extends Error {
  constructor(code, messageKey, details) {
    super(messageKey);
    this.code = code;
    this.messageKey = messageKey;
    this.details = details;
    this.name = "HaexVaultSdkError";
  }
  /**
   * Get localized error message
   * @param locale - Locale code (e.g., 'en', 'de')
   * @param translations - Translation object
   */
  getLocalizedMessage(locale = "en", translations) {
    if (!translations || !translations[locale]) {
      return this.messageKey;
    }
    let message = translations[locale][this.messageKey] || this.messageKey;
    if (this.details) {
      Object.entries(this.details).forEach(([key, value]) => {
        message = message.replace(`{${key}}`, String(value));
      });
    }
    return message;
  }
  toJSON() {
    return {
      code: this.code,
      message: this.messageKey,
      details: this.details
    };
  }
};
init_esm_shims();
init_esm_shims();
init_esm_shims();
init_esm_shims();
var DATABASE_COMMANDS = {
  /** Execute a SELECT query */
  query: "extension_database_query",
  /** Execute INSERT/UPDATE/DELETE statement */
  execute: "extension_database_execute",
  /** Execute multiple statements in a transaction */
  transaction: "extension_database_transaction",
  /** Register and apply database migrations */
  registerMigrations: "extension_database_register_migrations"
};
init_esm_shims();
var PERMISSIONS_COMMANDS = {
  /** Check web/fetch permission */
  checkWeb: "extension_permissions_check_web",
  /** Check database permission */
  checkDatabase: "extension_permissions_check_database",
  /** Check filesystem permission */
  checkFilesystem: "extension_permissions_check_filesystem"
};
init_esm_shims();
var WEB_COMMANDS = {
  /** Open URL in external browser */
  open: "extension_web_open",
  /** Make HTTP request (bypasses CORS) */
  fetch: "extension_web_fetch"
};
init_esm_shims();
var WEB_STORAGE_COMMANDS = {
  /** Get item from storage */
  getItem: "extension_web_storage_get_item",
  /** Set item in storage */
  setItem: "extension_web_storage_set_item",
  /** Remove item from storage */
  removeItem: "extension_web_storage_remove_item",
  /** Clear all storage */
  clear: "extension_web_storage_clear",
  /** Get all storage keys */
  keys: "extension_web_storage_keys"
};
init_esm_shims();
var FILESYSTEM_COMMANDS = {
  // Dialog commands
  /** Show save file dialog */
  saveFile: "extension_filesystem_save_file",
  /** Show open file dialog */
  openFile: "extension_filesystem_open_file",
  /** Show image in native viewer */
  showImage: "extension_filesystem_show_image",
  // Generic filesystem operations
  /** Read file contents as base64 */
  readFile: "extension_filesystem_read_file",
  /** Write base64 data to file */
  writeFile: "extension_filesystem_write_file",
  /** List directory contents */
  readDir: "extension_filesystem_read_dir",
  /** Create directory */
  mkdir: "extension_filesystem_mkdir",
  /** Remove file or directory */
  remove: "extension_filesystem_remove",
  /** Check if path exists */
  exists: "extension_filesystem_exists",
  /** Get file/directory metadata */
  stat: "extension_filesystem_stat",
  /** Show folder selection dialog */
  selectFolder: "extension_filesystem_select_folder",
  /** Show file selection dialog */
  selectFile: "extension_filesystem_select_file",
  /** Rename file or directory */
  rename: "extension_filesystem_rename",
  /** Copy file or directory */
  copy: "extension_filesystem_copy",
  /** Get well-known system directory paths */
  knownPaths: "extension_filesystem_known_paths",
  // File watcher operations
  /** Start watching a directory for changes */
  watch: "extension_filesystem_watch",
  /** Stop watching a directory */
  unwatch: "extension_filesystem_unwatch",
  /** Check if a directory is being watched */
  isWatching: "extension_filesystem_is_watching"
};
init_esm_shims();
var EXTERNAL_BRIDGE_COMMANDS = {
  // Response handling (called by extensions)
  /** Respond to an external request */
  respond: "external_bridge_respond",
  // Bridge server management
  /** Start the external bridge server */
  start: "external_bridge_start",
  /** Stop the external bridge server */
  stop: "external_bridge_stop",
  /** Get bridge server status */
  getStatus: "external_bridge_get_status",
  // Client authorization (unified API with remember flag)
  /** Allow a client connection */
  clientAllow: "external_bridge_client_allow",
  /** Block a client connection */
  clientBlock: "external_bridge_client_block",
  // Authorized clients management (permanent - stored in database)
  /** Get list of authorized clients */
  getAuthorizedClients: "external_bridge_get_authorized_clients",
  /** Revoke client authorization */
  revokeClient: "external_bridge_revoke_client",
  // Session-based authorizations (temporary - cleared when haex-vault restarts)
  /** Get session authorizations */
  getSessionAuthorizations: "external_bridge_get_session_authorizations",
  /** Revoke session authorization */
  revokeSessionAuthorization: "external_bridge_revoke_session_authorization",
  // Blocked clients management
  /** Get list of blocked clients */
  getBlockedClients: "external_bridge_get_blocked_clients",
  /** Unblock a client */
  unblockClient: "external_bridge_unblock_client",
  /** Check if client is blocked */
  isClientBlocked: "external_bridge_is_client_blocked",
  // Pending authorizations
  /** Get pending authorization requests */
  getPendingAuthorizations: "external_bridge_get_pending_authorizations"
};
init_esm_shims();
var EXTENSION_COMMANDS = {
  /** Get extension info (manifest, id, etc.) */
  getInfo: "extension_get_info",
  /** Get application context (theme, locale, etc.) */
  getContext: "extension_context_get",
  /** Set application context (host-only, updates stored context for extensions) */
  setContext: "extension_context_set",
  /** Broadcast event to ALL extension webview windows (host-only, desktop only) */
  webviewBroadcast: "extension_webview_broadcast",
  /** Emit event to all webviews of a SPECIFIC extension (host-only, desktop only) */
  webviewEmit: "extension_webview_emit",
  /** Filter sync tables by extension permissions (returns filtered map) */
  filterSyncTables: "extension_filter_sync_tables",
  /** Emit sync tables to webviews (host-only, desktop only) */
  emitSyncTables: "extension_emit_sync_tables",
  /** Ask the host to mark (or unmark) this extension's own tab as wanting attention */
  tabAttention: "extension_tab_attention",
  /** Ask the user to confirm, in a dialog the host draws over this extension's tab */
  dialogConfirm: "extension_dialog_confirm"
};
init_esm_shims();
var REMOTE_STORAGE_COMMANDS = {
  // Backend Management
  /** List all storage backends */
  listBackends: "extension_remote_storage_list_backends",
  /** Add a new storage backend */
  addBackend: "extension_remote_storage_add_backend",
  /** Update storage backend configuration */
  updateBackend: "extension_remote_storage_update_backend",
  /** Remove a storage backend */
  removeBackend: "extension_remote_storage_remove_backend",
  /** Test storage backend connection */
  testBackend: "extension_remote_storage_test_backend",
  // Storage Operations
  /** Upload data to storage backend */
  upload: "extension_remote_storage_upload",
  /** Download data from storage backend */
  download: "extension_remote_storage_download",
  /** Delete object from storage backend */
  delete: "extension_remote_storage_delete",
  /** List objects in storage backend */
  list: "extension_remote_storage_list"
};
init_esm_shims();
var ENCRYPTED_FOLDER_COMMANDS = {
  /** Let the user pick an encrypted folder of a storage and grant access to it */
  choose: "extension_encrypted_folder_choose",
  /** List the entries of a folder inside an encrypted folder */
  list: "extension_encrypted_folder_list",
  /** Download and decrypt a file */
  download: "extension_encrypted_folder_download",
  /** Encrypt and upload a file */
  upload: "extension_encrypted_folder_upload",
  /** Delete a file or folder */
  delete: "extension_encrypted_folder_delete"
};
init_esm_shims();
var LOCALSEND_COMMANDS = {
  // Initialization
  /** Initialize LocalSend (generate identity, etc.) */
  init: "localsend_init",
  /** Get our device info */
  getDeviceInfo: "localsend_get_device_info",
  /** Set our device alias */
  setAlias: "localsend_set_alias",
  // Settings
  /** Get current settings */
  getSettings: "localsend_get_settings",
  /** Update settings */
  setSettings: "localsend_set_settings",
  // Discovery (desktop only)
  /** Start device discovery via multicast UDP */
  startDiscovery: "localsend_start_discovery",
  /** Stop device discovery */
  stopDiscovery: "localsend_stop_discovery",
  /** Get list of discovered devices */
  getDevices: "localsend_get_devices",
  // Network scan (mobile only)
  /** Scan network for devices via HTTP */
  scanNetwork: "localsend_scan_network",
  // Server (receiving files)
  /** Start the HTTPS server for receiving files */
  startServer: "localsend_start_server",
  /** Stop the HTTPS server */
  stopServer: "localsend_stop_server",
  /** Get server status */
  getServerStatus: "localsend_get_server_status",
  /** Get pending incoming transfer requests */
  getPendingTransfers: "localsend_get_pending_transfers",
  /** Accept an incoming transfer */
  acceptTransfer: "localsend_accept_transfer",
  /** Reject an incoming transfer */
  rejectTransfer: "localsend_reject_transfer",
  // Client (sending files)
  /** Prepare files for sending - collect metadata */
  prepareFiles: "localsend_prepare_files",
  /** Send files to a device */
  sendFiles: "localsend_send_files",
  /** Cancel an outgoing transfer */
  cancelSend: "localsend_cancel_send"
};
init_esm_shims();
var SPACE_COMMANDS = {
  /** Bulk assign rows to spaces */
  assign: "extension_space_assign",
  /** Bulk unassign rows from spaces */
  unassign: "extension_space_unassign",
  /** Get space assignments for a table */
  getAssignments: "extension_space_get_assignments",
  /** Get members of a shared space */
  getMembers: "extension_space_get_members",
  /** List all spaces the user is a member of (with decrypted names) */
  list: "extension_space_list",
  /** Create a new shared space */
  create: "extension_space_create",
  /** List available sync backends */
  listBackends: "extension_space_list_backends"
};
init_esm_shims();
var SHELL_COMMANDS = {
  /** List available shell environments (no permission required) */
  listAvailable: "extension_shell_list_available",
  /** Create a new PTY shell session */
  create: "extension_shell_create",
  /** Write data to a shell session's stdin */
  write: "extension_shell_write",
  /** Acknowledge `shell:output` events handed on (backpressure; sent by the SDK itself) */
  ack: "extension_shell_ack",
  /** Resize a shell session's terminal */
  resize: "extension_shell_resize",
  /** Close a shell session */
  close: "extension_shell_close"
};
init_esm_shims();
var PASSWORD_COMMANDS = {
  /** List items (no secrets) within the extension's tag scope */
  list: "extension_password_list",
  /** Read full item including secrets, by id */
  read: "extension_password_read",
  /** Create item — must include >=1 tag in scope */
  create: "extension_password_create",
  /** Update item — keeps >=1 tag in scope */
  update: "extension_password_update",
  /** Delete item — must be in scope */
  delete: "extension_password_delete"
};
init_esm_shims();
var MAIL_COMMANDS = {
  /** LIST mailboxes + optional STATUS counts */
  listMailboxes: "extension_mail_list_mailboxes",
  /** Lightweight envelope fetch for list views */
  fetchEnvelopes: "extension_mail_fetch_envelopes",
  /** Full message fetch (envelope + body + attachment metadata) */
  fetchMessage: "extension_mail_fetch_message",
  /** Fetch a single attachment's bytes (base64) by part index */
  fetchAttachment: "extension_mail_fetch_attachment",
  /** Set or unset IMAP flags on a UID set */
  setFlags: "extension_mail_set_flags",
  /** MOVE messages between mailboxes (COPY+EXPUNGE fallback) */
  moveMessages: "extension_mail_move_messages",
  /** APPEND a base64-encoded RFC822 message into a mailbox */
  appendMessage: "extension_mail_append_message",
  /** SMTP send. Returns the assigned Message-ID. */
  sendMessage: "extension_mail_send_message",
  /** Build RFC822 bytes without sending (for Drafts via APPEND) */
  buildRfc822: "extension_mail_build_rfc822",
  /** Start (or replace) a background new-mail watch for an account/mailbox */
  startWatch: "extension_mail_start_watch",
  /** Stop a background new-mail watch */
  stopWatch: "extension_mail_stop_watch"
};
init_esm_shims();
var NOTIFICATION_COMMANDS = {
  /** Show an OS notification. Returns the assigned notification id. */
  show: "extension_notifications_show",
  /** Dismiss a previously shown notification (only own notifications). */
  dismiss: "extension_notifications_dismiss"
};
var StorageAPI = class {
  constructor(client) {
    this.client = client;
  }
  async getItem(key) {
    return this.client.request(WEB_STORAGE_COMMANDS.getItem, { key });
  }
  async setItem(key, value) {
    await this.client.request(WEB_STORAGE_COMMANDS.setItem, { key, value });
  }
  async removeItem(key) {
    await this.client.request(WEB_STORAGE_COMMANDS.removeItem, { key });
  }
  async clear() {
    await this.client.request(WEB_STORAGE_COMMANDS.clear);
  }
  async keys() {
    return this.client.request(WEB_STORAGE_COMMANDS.keys);
  }
};
init_esm_shims();
function quoteIdent(identifier) {
  if (identifier.startsWith('"') && identifier.endsWith('"')) {
    return identifier;
  }
  return `"${identifier.replace(/"/g, '""')}"`;
}
var DatabaseAPI = class {
  constructor(client) {
    this.client = client;
  }
  async query(query, params) {
    const result = await this.client.request(
      DATABASE_COMMANDS.query,
      {
        query,
        params: params || []
      }
    );
    return result.rows;
  }
  async queryOne(query, params) {
    const rows = await this.query(query, params);
    return rows.length > 0 ? rows[0] ?? null : null;
  }
  async execute(query, params) {
    return this.client.request(DATABASE_COMMANDS.execute, {
      query,
      params: params || []
    });
  }
  async transaction(statements) {
    await this.client.request(DATABASE_COMMANDS.transaction, {
      statements: statements.map((s) => [s.sql, s.params || []])
    });
  }
  async createTable(tableName, columns) {
    const query = `CREATE TABLE IF NOT EXISTS ${quoteIdent(tableName)} (${columns})`;
    await this.execute(query);
  }
  async dropTable(tableName) {
    const query = `DROP TABLE IF EXISTS ${quoteIdent(tableName)}`;
    await this.execute(query);
  }
  /**
   * Registers and applies extension migrations with HaexVault
   *
   * HaexVault will:
   * 1. Validate all SQL statements (ensure only extension's own tables are accessed)
   * 2. Store migrations with applied_at = NULL
   * 3. Query pending migrations sorted by name
   * 4. Apply pending migrations and set up CRDT triggers
   * 5. Mark successful migrations with applied_at timestamp
   *
   * @param extensionVersion - The version of the extension
   * @param migrations - Array of migration objects with name and SQL content
   * @returns Promise with migration result (applied count, already applied count, applied migration names)
   */
  async registerMigrationsAsync(extensionVersion, migrations) {
    return this.client.request(
      DATABASE_COMMANDS.registerMigrations,
      {
        extensionVersion,
        migrations
      }
    );
  }
  async insert(tableName, data) {
    const keys = Object.keys(data);
    const values = Object.values(data);
    const quotedCols = keys.map(quoteIdent).join(", ");
    const placeholders = keys.map(() => "?").join(", ");
    const query = `INSERT INTO ${quoteIdent(tableName)} (${quotedCols}) VALUES (${placeholders})`;
    const result = await this.execute(query, values);
    return result.lastInsertId ?? -1;
  }
  async update(tableName, data, where, whereParams) {
    const keys = Object.keys(data);
    const values = Object.values(data);
    const setClause = keys.map((key) => `${quoteIdent(key)} = ?`).join(", ");
    const query = `UPDATE ${quoteIdent(tableName)} SET ${setClause} WHERE ${where}`;
    const result = await this.execute(query, [
      ...values,
      ...whereParams || []
    ]);
    return result.rowsAffected;
  }
  async delete(tableName, where, whereParams) {
    const query = `DELETE FROM ${quoteIdent(tableName)} WHERE ${where}`;
    const result = await this.execute(query, whereParams);
    return result.rowsAffected;
  }
  async count(tableName, where, whereParams) {
    const query = where ? `SELECT COUNT(*) as count FROM ${quoteIdent(tableName)} WHERE ${where}` : `SELECT COUNT(*) as count FROM ${quoteIdent(tableName)}`;
    const result = await this.queryOne(query, whereParams);
    return result?.count ?? 0;
  }
};
init_esm_shims();
init_vaultKey();
var FilesystemAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Opens a save file dialog and saves the provided data to the selected location
   * @param data The file data as Uint8Array
   * @param options Options for the save dialog
   * @returns The path where the file was saved, or null if cancelled
   */
  async saveFileAsync(data, options = {}) {
    const result = await this.client.request(
      FILESYSTEM_COMMANDS.saveFile,
      {
        data: Array.from(data),
        // Convert Uint8Array to regular array for postMessage
        defaultPath: options.defaultPath,
        title: options.title,
        filters: options.filters
      }
    );
    return result;
  }
  /**
   * Opens a file with the system's default viewer
   * @param data The file data as Uint8Array
   * @param options Options for opening the file
   * @returns The result of the operation
   */
  async openFileAsync(data, options) {
    const result = await this.client.request(
      FILESYSTEM_COMMANDS.openFile,
      {
        data: Array.from(data),
        // Convert Uint8Array to regular array for postMessage
        fileName: options.fileName,
        mimeType: options.mimeType
      }
    );
    return result;
  }
  /**
   * Shows an image using a data URL (safe, read-only viewing)
   * This is safe to use without special permissions as it only displays images
   * and doesn't execute any code or open files with external applications
   * @param options Options containing the data URL
   * @returns The result of the operation
   */
  async showImageAsync(options) {
    const result = await this.client.request(
      FILESYSTEM_COMMANDS.showImage,
      {
        dataUrl: options.dataUrl
      }
    );
    return result;
  }
  // ==========================================================================
  // Generic Filesystem Operations (Phase 2)
  // ==========================================================================
  /**
   * Read file contents
   * @param path Absolute path to the file
   * @returns File contents as Uint8Array
   */
  async readFile(path2) {
    const base64 = await this.client.request(
      FILESYSTEM_COMMANDS.readFile,
      { path: path2 }
    );
    return base64ToArrayBuffer(base64);
  }
  /**
   * Write file contents
   * @param path Absolute path to the file
   * @param data File contents as Uint8Array
   */
  async writeFile(path2, data) {
    const base64 = arrayBufferToBase64(data);
    await this.client.request(
      FILESYSTEM_COMMANDS.writeFile,
      { path: path2, data: base64 }
    );
  }
  /**
   * Read directory contents
   * @param path Absolute path to the directory
   * @returns Array of directory entries
   */
  async readDir(path2) {
    return this.client.request(
      FILESYSTEM_COMMANDS.readDir,
      { path: path2 }
    );
  }
  /**
   * Create a directory (and parent directories if needed)
   * @param path Absolute path to create
   */
  async mkdir(path2) {
    await this.client.request(
      FILESYSTEM_COMMANDS.mkdir,
      { path: path2 }
    );
  }
  /**
   * Remove a file or directory
   * @param path Absolute path to remove
   * @param recursive If true, remove directories recursively
   */
  async remove(path2, recursive = false) {
    await this.client.request(
      FILESYSTEM_COMMANDS.remove,
      { path: path2, recursive }
    );
  }
  /**
   * Check if a path exists
   * @param path Absolute path to check
   * @returns True if the path exists
   */
  async exists(path2) {
    return this.client.request(
      FILESYSTEM_COMMANDS.exists,
      { path: path2 }
    );
  }
  /**
   * Get file/directory metadata
   * @param path Absolute path
   * @returns File metadata
   */
  async stat(path2) {
    return this.client.request(
      FILESYSTEM_COMMANDS.stat,
      { path: path2 }
    );
  }
  /**
   * Open a folder selection dialog
   * @param options Dialog options
   * @returns Selected folder path, or null if cancelled
   */
  async selectFolder(options = {}) {
    return this.client.request(
      FILESYSTEM_COMMANDS.selectFolder,
      options
    );
  }
  /**
   * Open a file selection dialog
   * @param options Dialog options
   * @returns Selected file paths, or null if cancelled
   */
  async selectFile(options = {}) {
    return this.client.request(
      FILESYSTEM_COMMANDS.selectFile,
      options
    );
  }
  /**
   * Rename/move a file or directory
   * @param from Source path
   * @param to Destination path
   */
  async rename(from, to) {
    await this.client.request(
      FILESYSTEM_COMMANDS.rename,
      { from, to }
    );
  }
  /**
   * Copy a file
   * @param from Source path
   * @param to Destination path
   */
  async copy(from, to) {
    await this.client.request(
      FILESYSTEM_COMMANDS.copy,
      { from, to }
    );
  }
  // ==========================================================================
  // Known Paths (System Directories)
  // ==========================================================================
  /**
   * Get well-known system directory paths (home, pictures, downloads, etc.)
   * These paths are resolved via Tauri's PathResolver and are platform-aware.
   * @returns Map of known path names to their absolute paths
   */
  async knownPaths() {
    return this.client.request(
      FILESYSTEM_COMMANDS.knownPaths
    );
  }
  // ==========================================================================
  // File Watcher Operations
  // ==========================================================================
  /**
   * Start watching a directory for file changes
   * When files change, the "filesync:file-changed" event is emitted
   * @param ruleId Unique identifier for this watch (e.g., sync rule ID)
   * @param path Absolute path to the directory to watch
   */
  async watch(ruleId, path2) {
    await this.client.request(
      FILESYSTEM_COMMANDS.watch,
      { ruleId, path: path2 }
    );
  }
  /**
   * Stop watching a directory
   * @param ruleId The rule ID that was used when starting the watch
   */
  async unwatch(ruleId) {
    await this.client.request(
      FILESYSTEM_COMMANDS.unwatch,
      { ruleId }
    );
  }
  /**
   * Check if a directory is currently being watched
   * @param ruleId The rule ID to check
   * @returns True if the directory is being watched
   */
  async isWatching(ruleId) {
    return this.client.request(
      FILESYSTEM_COMMANDS.isWatching,
      { ruleId }
    );
  }
};
init_esm_shims();
var WebAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Performs a web request through the HaexHub host application
   * @param url The URL to fetch
   * @param options Request options (method, headers, body, timeout)
   * @returns Promise resolving to the web response
   */
  async fetchAsync(url, options = {}) {
    let bodyParam;
    if (options.body) {
      if (options.body instanceof ArrayBuffer) {
        bodyParam = this.arrayBufferToBase64(options.body);
      } else if (options.body instanceof Blob) {
        bodyParam = await this.blobToBase64(options.body);
      } else {
        const bytes = new TextEncoder().encode(options.body);
        bodyParam = this.arrayBufferToBase64(
          bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)
        );
      }
    }
    const response = await this.client.request(WEB_COMMANDS.fetch, {
      url,
      method: options.method || "GET",
      headers: options.headers,
      body: bodyParam,
      timeout: options.timeout
    });
    const bodyBuffer = this.base64ToArrayBuffer(response.body);
    return {
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
      body: bodyBuffer,
      url: response.url
    };
  }
  /**
   * Convenience method for JSON requests
   */
  async fetchJsonAsync(url, options = {}) {
    const response = await this.fetchAsync(url, options);
    const text = new TextDecoder().decode(response.body);
    return JSON.parse(text);
  }
  /**
   * Convenience method for text requests
   */
  async fetchTextAsync(url, options = {}) {
    const response = await this.fetchAsync(url, options);
    return new TextDecoder().decode(response.body);
  }
  /**
   * Convenience method for blob requests
   */
  async fetchBlobAsync(url, options = {}) {
    const response = await this.fetchAsync(url, options);
    return new Blob([response.body]);
  }
  /**
   * Opens a URL in the user's default browser
   * @param url The URL to open
   */
  async openAsync(url) {
    await this.client.request(WEB_COMMANDS.open, { url });
  }
  arrayBufferToBase64(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = "";
    for (let i = 0; i < bytes.byteLength; i++) {
      const byte = bytes[i];
      if (byte === void 0) {
        throw new Error("Invalid byte at index " + i);
      }
      binary += String.fromCharCode(byte);
    }
    return btoa(binary);
  }
  async blobToBase64(blob) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onloadend = () => {
        const result = reader.result;
        if (typeof result !== "string") {
          reject(new Error("Failed to read blob as data URL"));
          return;
        }
        const parts = result.split(",");
        const base64 = parts[1];
        if (!base64) {
          reject(new Error("Failed to extract base64 from data URL"));
          return;
        }
        resolve(base64);
      };
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
  }
  base64ToArrayBuffer(base64) {
    const binaryString = atob(base64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes.buffer;
  }
};
init_esm_shims();
var PermissionsAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Checks if the extension has permission for a database operation
   * @param resource The database resource (table name or "*" for all tables)
   * @param operation The operation type ("read" or "write")
   * @returns Promise<boolean> indicating if permission is granted
   */
  async checkDatabaseAsync(resource, operation) {
    const response = await this.client.request(
      PERMISSIONS_COMMANDS.checkDatabase,
      {
        resource,
        operation
      }
    );
    return response.status === "granted";
  }
  /**
   * Checks if the extension has permission for a web request
   * @param url The URL to check (e.g., "https://example.com/path")
   * @returns Promise<boolean> indicating if permission is granted
   * @note Method/operation is not checked - permissions apply to all HTTP methods
   */
  async checkWebAsync(url) {
    const response = await this.client.request(
      PERMISSIONS_COMMANDS.checkWeb,
      {
        url
      }
    );
    return response.status === "granted";
  }
  /**
   * Checks if the extension has permission for a filesystem operation
   * @param path The file or directory path
   * @param operation The operation type ("read" or "write")
   * @returns Promise<boolean> indicating if permission is granted
   */
  async checkFilesystemAsync(path2, operation) {
    const response = await this.client.request(
      PERMISSIONS_COMMANDS.checkFilesystem,
      {
        path: path2,
        operation
      }
    );
    return response.status === "granted";
  }
};
init_esm_shims();
init_vaultKey();
var RemoteStorageAPI = class {
  constructor(client) {
    this.client = client;
    this.backends = new BackendManagement(client);
  }
  /**
   * Upload data to a storage backend
   * @param backendId - Backend ID to upload to
   * @param key - Object key (path in the bucket)
   * @param data - Data to upload
   */
  async upload(backendId, key, data) {
    const base64 = arrayBufferToBase64(data);
    await this.client.request(REMOTE_STORAGE_COMMANDS.upload, {
      request: { backendId, key, data: base64 }
    });
  }
  /**
   * Download data from a storage backend
   * @param backendId - Backend ID to download from
   * @param key - Object key (path in the bucket)
   * @returns Downloaded data as Uint8Array
   */
  async download(backendId, key) {
    const base64 = await this.client.request(
      REMOTE_STORAGE_COMMANDS.download,
      { request: { backendId, key } }
    );
    return base64ToArrayBuffer(base64);
  }
  /**
   * Delete an object from a storage backend
   * @param backendId - Backend ID
   * @param key - Object key to delete
   */
  async delete(backendId, key) {
    await this.client.request(REMOTE_STORAGE_COMMANDS.delete, {
      request: { backendId, key }
    });
  }
  /**
   * List objects in a storage backend
   * @param backendId - Backend ID
   * @param prefix - Optional prefix to filter objects
   * @returns List of objects
   */
  async list(backendId, prefix) {
    return this.client.request(
      REMOTE_STORAGE_COMMANDS.list,
      { request: { backendId, prefix } }
    );
  }
};
var BackendManagement = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * List all available storage backends
   */
  async list() {
    return this.client.request(
      REMOTE_STORAGE_COMMANDS.listBackends
    );
  }
  /**
   * Propose a new storage. The host asks the user; credentials are entered
   * only there. Resolves with the new storage, which the extension may then
   * read and write.
   * @param request - The proposal, without credentials
   * @returns Created backend info
   */
  async add(request) {
    return this.client.request(
      REMOTE_STORAGE_COMMANDS.addBackend,
      { request },
      // No deadline: the host waits for the user's answer in its dialog.
      { timeout: null }
    );
  }
  /**
   * Change a storage after the user confirms it in the host.
   * Only provided fields are updated; new credentials are entered in the host.
   * @param request - Update request with backendId and fields to update
   * @returns Updated backend info
   */
  async update(request) {
    return this.client.request(
      REMOTE_STORAGE_COMMANDS.updateBackend,
      { request },
      // No deadline: the host waits for the user's answer in its dialog.
      { timeout: null }
    );
  }
  /**
   * Remove a storage backend
   * @param backendId - Backend ID to remove
   */
  async remove(backendId) {
    await this.client.request(
      REMOTE_STORAGE_COMMANDS.removeBackend,
      { backendId },
      { timeout: null }
    );
  }
  /**
   * Test connection to a storage backend
   * @param backendId - Backend ID to test
   */
  async test(backendId) {
    await this.client.request(
      REMOTE_STORAGE_COMMANDS.testBackend,
      { backendId },
      { timeout: null }
    );
  }
};
init_esm_shims();
init_vaultKey();
var EncryptedFoldersAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Let the user choose one of the storage's encrypted folders and grant
   * access to it. The host shows its own window and waits for the answer.
   * Needs the `remoteStorage` permission for the storage (else 1002 without
   * asking); 1002 as well when the user refuses.
   * @param storageId - Storage the folder lies in
   * @param access - Access the extension asks for
   * @returns The chosen folder, or `null` when the user closed the chooser without choosing
   */
  async choose(storageId, access) {
    const choice = await this.client.request(
      ENCRYPTED_FOLDER_COMMANDS.choose,
      { request: { storageId, access } },
      // No deadline: the host waits for the user's answer in its window.
      { timeout: null }
    );
    return choice ?? null;
  }
  /**
   * List a folder inside an encrypted folder (permission `read`).
   * @param storageId - Storage the folder lies in
   * @param folderId - Folder from {@link EncryptedFoldersAPI.choose}
   * @param path - Path relative to the encrypted folder; `''` (default) for its root
   * @returns The entries with their decrypted names
   */
  async list(storageId, folderId, path2 = "") {
    const result = await this.client.request(
      ENCRYPTED_FOLDER_COMMANDS.list,
      { request: { storageId, folderId, path: path2 } }
    );
    return result.entries;
  }
  /**
   * Download and decrypt a file (permission `read`).
   * @param storageId - Storage the folder lies in
   * @param folderId - Folder from {@link EncryptedFoldersAPI.choose}
   * @param path - Path of the file, relative to the encrypted folder
   * @returns The decrypted content
   */
  async download(storageId, folderId, path2) {
    const base64 = await this.client.request(
      ENCRYPTED_FOLDER_COMMANDS.download,
      { request: { storageId, folderId, path: path2 } }
    );
    return base64ToArrayBuffer(base64);
  }
  /**
   * Encrypt and upload a file (permission `readWrite`). Missing parent
   * folders are created.
   * @param storageId - Storage the folder lies in
   * @param folderId - Folder from {@link EncryptedFoldersAPI.choose}
   * @param path - Path of the file, relative to the encrypted folder
   * @param data - Content of the file
   */
  async upload(storageId, folderId, path2, data) {
    await this.client.request(ENCRYPTED_FOLDER_COMMANDS.upload, {
      request: { storageId, folderId, path: path2, data: arrayBufferToBase64(data) }
    });
  }
  /**
   * Delete a file or folder (permission `readWrite`).
   * @param storageId - Storage the folder lies in
   * @param folderId - Folder from {@link EncryptedFoldersAPI.choose}
   * @param path - Path relative to the encrypted folder
   */
  async delete(storageId, folderId, path2) {
    await this.client.request(ENCRYPTED_FOLDER_COMMANDS.delete, {
      request: { storageId, folderId, path: path2 }
    });
  }
};
init_esm_shims();
var LOCALSEND_EVENTS = {
  /** New device discovered */
  deviceDiscovered: "localsend:device-discovered",
  /** Device lost (no longer visible) */
  deviceLost: "localsend:device-lost",
  /** New incoming transfer request */
  transferRequest: "localsend:transfer-request",
  /** Transfer progress update */
  transferProgress: "localsend:transfer-progress",
  /** Transfer completed */
  transferComplete: "localsend:transfer-complete",
  /** Transfer failed */
  transferFailed: "localsend:transfer-failed"
};
var LocalSendAPI = class {
  constructor(client) {
    this.client = client;
  }
  // ==========================================================================
  // Initialization
  // ==========================================================================
  /**
   * Initialize LocalSend (generate identity, etc.)
   * Call this on app start
   * @returns Our device info
   */
  async init() {
    return this.client.request(LOCALSEND_COMMANDS.init, {});
  }
  /**
   * Get our device info
   * @returns Our device info
   */
  async getDeviceInfo() {
    return this.client.request(LOCALSEND_COMMANDS.getDeviceInfo, {});
  }
  /**
   * Set our device alias (display name)
   * @param alias New alias
   */
  async setAlias(alias) {
    await this.client.request(LOCALSEND_COMMANDS.setAlias, { alias });
  }
  // ==========================================================================
  // Settings
  // ==========================================================================
  /**
   * Get current LocalSend settings
   * @returns Settings
   */
  async getSettings() {
    return this.client.request(LOCALSEND_COMMANDS.getSettings, {});
  }
  /**
   * Update LocalSend settings
   * @param settings New settings
   */
  async setSettings(settings) {
    await this.client.request(LOCALSEND_COMMANDS.setSettings, { settings });
  }
  // ==========================================================================
  // Discovery (Desktop only - multicast UDP)
  // ==========================================================================
  /**
   * Start device discovery via multicast UDP
   * Desktop only - on mobile use scanNetwork()
   */
  async startDiscovery() {
    await this.client.request(LOCALSEND_COMMANDS.startDiscovery, {});
  }
  /**
   * Stop device discovery
   * Desktop only
   */
  async stopDiscovery() {
    await this.client.request(LOCALSEND_COMMANDS.stopDiscovery, {});
  }
  /**
   * Get list of discovered devices
   * @returns Array of discovered devices
   */
  async getDevices() {
    return this.client.request(LOCALSEND_COMMANDS.getDevices, {});
  }
  // ==========================================================================
  // Network Scan (Mobile only - HTTP)
  // ==========================================================================
  /**
   * Scan network for devices via HTTP
   * Mobile only - on desktop use startDiscovery()
   * @returns Array of discovered devices
   */
  async scanNetwork() {
    return this.client.request(LOCALSEND_COMMANDS.scanNetwork, {});
  }
  // ==========================================================================
  // Server (Receiving files)
  // ==========================================================================
  /**
   * Start the HTTPS server for receiving files
   * @param port Optional port to listen on (default: 53317)
   * @returns Server info with port, fingerprint, and addresses
   */
  async startServer(port) {
    return this.client.request(LOCALSEND_COMMANDS.startServer, { port });
  }
  /**
   * Stop the HTTPS server
   */
  async stopServer() {
    await this.client.request(LOCALSEND_COMMANDS.stopServer, {});
  }
  /**
   * Get server status
   * @returns Server status
   */
  async getServerStatus() {
    return this.client.request(LOCALSEND_COMMANDS.getServerStatus, {});
  }
  /**
   * Get pending incoming transfer requests
   * @returns Array of pending transfers
   */
  async getPendingTransfers() {
    return this.client.request(LOCALSEND_COMMANDS.getPendingTransfers, {});
  }
  /**
   * Accept an incoming transfer
   * @param sessionId Session ID of the transfer
   * @param saveDir Directory to save files to
   */
  async acceptTransfer(sessionId, saveDir) {
    await this.client.request(LOCALSEND_COMMANDS.acceptTransfer, { sessionId, saveDir });
  }
  /**
   * Reject an incoming transfer
   * @param sessionId Session ID of the transfer
   */
  async rejectTransfer(sessionId) {
    await this.client.request(LOCALSEND_COMMANDS.rejectTransfer, { sessionId });
  }
  // ==========================================================================
  // Client (Sending files)
  // ==========================================================================
  /**
   * Prepare files for sending - collect metadata
   * @param paths Array of file/folder paths to send
   * @returns Array of file info with metadata
   */
  async prepareFiles(paths) {
    return this.client.request(LOCALSEND_COMMANDS.prepareFiles, { paths });
  }
  /**
   * Send files to a device
   * @param device Target device
   * @param files Files to send (from prepareFiles)
   * @returns Session ID for tracking progress
   */
  async sendFiles(device, files) {
    return this.client.request(LOCALSEND_COMMANDS.sendFiles, { device, files });
  }
  /**
   * Cancel an outgoing transfer
   * @param sessionId Session ID of the transfer
   */
  async cancelSend(sessionId) {
    await this.client.request(LOCALSEND_COMMANDS.cancelSend, { sessionId });
  }
};
init_esm_shims();
var SpacesAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Bulk assign rows to spaces.
   * @param assignments - Array of row-to-space mappings
   * @returns Number of assignments created
   */
  async assignAsync(assignments) {
    return this.client.request(SPACE_COMMANDS.assign, { assignments });
  }
  /**
   * Bulk unassign rows from spaces.
   * @param assignments - Array of row-to-space mappings to remove
   * @returns Number of assignments removed
   */
  async unassignAsync(assignments) {
    return this.client.request(SPACE_COMMANDS.unassign, { assignments });
  }
  /**
   * Get space assignments for a table, optionally filtered by row.
   * @param tableName - The table to query assignments for
   * @param rowPks - Optional row primary key(s) to filter by
   * @returns Array of space assignments
   */
  async getAssignmentsAsync(tableName, rowPks) {
    return this.client.request(
      SPACE_COMMANDS.getAssignments,
      { tableName, rowPks: rowPks ? [rowPks] : void 0 }
    );
  }
  /**
   * Convenience method to assign a single row to a space.
   * @param tableName - The table name
   * @param rowPks - The row primary key(s)
   * @param spaceId - The space ID to assign to
   * @returns Number of assignments created (0 or 1)
   */
  async assignRowAsync(tableName, rowPks, spaceId) {
    return this.assignAsync([{ tableName, rowPks, spaceId }]);
  }
  /**
   * Convenience method to unassign a single row from a space.
   * @param tableName - The table name
   * @param rowPks - The row primary key(s)
   * @param spaceId - The space ID to unassign from
   * @returns Number of assignments removed (0 or 1)
   */
  async unassignRowAsync(tableName, rowPks, spaceId) {
    return this.unassignAsync([{ tableName, rowPks, spaceId }]);
  }
  // ==========================================================================
  // Space Management
  // ==========================================================================
  /**
   * List all shared spaces the user is a member of.
   * Returns spaces with decrypted names (decryption happens vault-side).
   */
  async listSpacesAsync() {
    return this.client.request(SPACE_COMMANDS.list);
  }
  /**
   * List members of a shared space (DID + label), flagging the current user.
   * Used to resolve assignment authors to names and to detect own vs shared-in content.
   */
  async getMembersAsync(spaceId) {
    return this.client.request(SPACE_COMMANDS.getMembers, { spaceId });
  }
};
init_esm_shims();
var NOT_SUPPORTED = 8e3;
function isUnsupported(error) {
  if (error?.code === NOT_SUPPORTED) return true;
  return typeof error === "string" && /\bcommand (\S+ )?not found\b/i.test(error);
}
var ShellAPI = class {
  constructor(sdk2) {
    this.sdk = sdk2;
    this.unacknowledged = /* @__PURE__ */ new Map();
    this.flushScheduled = false;
    this.ackUnsupported = false;
    this.sdk.on(SHELL_EVENTS.OUTPUT, (event) => this.acknowledge(event));
  }
  /**
   * Backpressure: the host keeps reading a shell's output only while this frame has few
   * events open, so a shell that writes fast waits instead of flooding the page. The SDK
   * acknowledges every event once the listeners have had it, in one request per session and
   * task.
   */
  acknowledge(event) {
    const sessionId = event.data?.sessionId;
    if (this.ackUnsupported || typeof sessionId !== "string") return;
    this.unacknowledged.set(sessionId, (this.unacknowledged.get(sessionId) ?? 0) + 1);
    if (this.flushScheduled) return;
    this.flushScheduled = true;
    queueMicrotask(() => this.flushAcknowledgements());
  }
  flushAcknowledgements() {
    this.flushScheduled = false;
    const counts = [...this.unacknowledged];
    this.unacknowledged.clear();
    for (const [sessionId, count] of counts) {
      this.sdk.request(SHELL_COMMANDS.ack, { sessionId, count }).catch((error) => {
        if (isUnsupported(error)) this.ackUnsupported = true;
      });
    }
  }
  /**
   * List available shell environments on the host system.
   * No filesystem permission required.
   */
  async listAvailable() {
    return await this.sdk.request(SHELL_COMMANDS.listAvailable, {});
  }
  /**
   * Create a new PTY shell session.
   * Returns a session ID used for subsequent write/resize/close operations.
   * Listen for shell output via `sdk.shell.onData()`.
   */
  async create(options = {}) {
    return await this.sdk.request(
      SHELL_COMMANDS.create,
      { options }
    );
  }
  /**
   * Write data to a shell session's stdin.
   * Typically used to forward terminal keystrokes.
   */
  async write(sessionId, data) {
    await this.sdk.request(SHELL_COMMANDS.write, { sessionId, data });
  }
  /**
   * Resize a shell session's terminal.
   * Should be called when the terminal view is resized.
   */
  async resize(sessionId, cols, rows) {
    await this.sdk.request(SHELL_COMMANDS.resize, { sessionId, cols, rows });
  }
  /**
   * Close a shell session.
   * This terminates the underlying PTY process.
   */
  async close(sessionId) {
    await this.sdk.request(SHELL_COMMANDS.close, { sessionId });
  }
  /**
   * Register a callback for shell output data.
   * The callback receives output from the PTY's stdout/stderr.
   */
  onData(callback) {
    this.sdk.on("shell:output", callback);
  }
  /**
   * Register a callback for shell session exit.
   */
  onExit(callback) {
    this.sdk.on("shell:exit", callback);
  }
  /**
   * Remove a shell output callback.
   */
  offData(callback) {
    this.sdk.off("shell:output", callback);
  }
  /**
   * Remove a shell exit callback.
   */
  offExit(callback) {
    this.sdk.off("shell:exit", callback);
  }
};
init_esm_shims();
var PasswordsAPI = class {
  constructor(client) {
    this.client = client;
  }
  /** List items in scope — summaries only, no secrets. */
  async listAsync() {
    return this.client.request(
      PASSWORD_COMMANDS.list,
      {}
    );
  }
  /** Read a single item by id with full secrets. */
  async readAsync(itemId) {
    return this.client.request(PASSWORD_COMMANDS.read, {
      itemId
    });
  }
  /**
   * Create a new password item. `input.tags` must contain at least one
   * tag within the extension's permission scope, otherwise the write
   * is rejected as a security violation.
   *
   * Returns the new item id.
   */
  async createAsync(input) {
    return this.client.request(PASSWORD_COMMANDS.create, { input });
  }
  /**
   * Update an existing item. The item must already be in scope, and
   * the new tag set must keep at least one tag in scope (extensions
   * cannot orphan an item out of their own reach).
   */
  async updateAsync(itemId, input) {
    return this.client.request(PASSWORD_COMMANDS.update, {
      itemId,
      input
    });
  }
  /** Delete an item by id. Item must be in scope. */
  async deleteAsync(itemId) {
    return this.client.request(PASSWORD_COMMANDS.delete, { itemId });
  }
};
init_esm_shims();
var MailAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * LIST mailboxes for an IMAP account. Pass `includeStatus=true` for
   * EXISTS/UNSEEN/UIDVALIDITY/UIDNEXT per box (one extra round-trip
   * per mailbox — fine for typical accounts, expensive for large
   * trees).
   */
  async listMailboxesAsync(imap, options = {}) {
    return this.client.request(MAIL_COMMANDS.listMailboxes, {
      imap,
      reference: options.reference,
      pattern: options.pattern,
      includeStatus: options.includeStatus
    });
  }
  /** Fetch lightweight envelopes for a mailbox + range (for list views). */
  async fetchEnvelopesAsync(imap, mailbox, range) {
    return this.client.request(
      MAIL_COMMANDS.fetchEnvelopes,
      { imap, mailbox, range }
    );
  }
  /** Fetch a full message (envelope + body + attachment metadata) by UID. */
  async fetchMessageAsync(imap, mailbox, uid) {
    return this.client.request(MAIL_COMMANDS.fetchMessage, {
      imap,
      mailbox,
      uid
    });
  }
  /**
   * Fetch a single attachment's raw bytes by its `partIndex` (from the
   * `attachments` array of a fetched `MailMessage`), returned as a
   * standard-alphabet base64 string. The base64 form drops straight into
   * `OutgoingAttachment.data` when forwarding, and decodes to bytes for
   * viewing or downloading.
   */
  async fetchAttachmentAsync(imap, mailbox, uid, partIndex) {
    return this.client.request(MAIL_COMMANDS.fetchAttachment, {
      imap,
      mailbox,
      uid,
      partIndex
    });
  }
  /**
   * Set or unset IMAP flags. Use `flags=["\\Seen"]` + `add=true` to
   * mark messages as read; `add=false` removes the flag(s).
   */
  async setFlagsAsync(imap, mailbox, uids, flags, add) {
    return this.client.request(MAIL_COMMANDS.setFlags, {
      imap,
      mailbox,
      uids,
      flags,
      add
    });
  }
  /** Move messages between mailboxes. Falls back to COPY+EXPUNGE on servers without MOVE. */
  async moveMessagesAsync(imap, sourceMailbox, destinationMailbox, uids) {
    return this.client.request(MAIL_COMMANDS.moveMessages, {
      imap,
      sourceMailbox,
      destinationMailbox,
      uids
    });
  }
  /**
   * APPEND a base64-encoded RFC822 message into a mailbox. Combine
   * with `buildRfc822Async` to save drafts, or with the bytes returned
   * after `sendMessageAsync` to mirror the sent copy into "Sent".
   */
  async appendMessageAsync(imap, mailbox, rfc822Base64, flags) {
    return this.client.request(MAIL_COMMANDS.appendMessage, {
      imap,
      mailbox,
      rfc822Base64,
      flags
    });
  }
  /** Send a message via SMTP. Returns the assigned Message-ID (no angle brackets). */
  async sendMessageAsync(smtp, message) {
    return this.client.request(MAIL_COMMANDS.sendMessage, {
      smtp,
      message
    });
  }
  /**
   * Build RFC822 bytes for a message without sending — useful for
   * drafts that get APPENDed to a "Drafts" folder. Permission-wise
   * this is a fetch operation (no SMTP host involved).
   */
  async buildRfc822Async(imapHost, message) {
    return this.client.request(MAIL_COMMANDS.buildRfc822, {
      imapHost,
      message
    });
  }
  /**
   * Start (or replace) a background watch for `accountId`/`mailboxName`.
   * Requires `mail` permission with action `poll` on the account's IMAP
   * host (`imap.host` when passed; prompted on first call, same as
   * `fetch`/`send`).
   *
   * Pass the account's `imap` config, as for every other mail call: the host
   * keeps the credentials in memory for as long as the watch runs and never
   * stores them. Hosts that wait with IMAP IDLE are told about new mail by the
   * server; `intervalSeconds` (clamped host-side to [30, 3600]) is the polling
   * interval for servers without IDLE. Listen for results via `onNewMessages`.
   *
   * `imap` is optional for hosts that resolve the credentials themselves from
   * `accountId`; holzi requires it.
   */
  async startWatchingAsync(accountId, mailboxName, intervalSeconds, imap) {
    return this.client.request(MAIL_COMMANDS.startWatch, {
      accountId,
      mailboxName,
      intervalSeconds,
      imap
    });
  }
  /** Stop a background watch previously started with `startWatchingAsync`. */
  async stopWatchingAsync(accountId, mailboxName) {
    return this.client.request(MAIL_COMMANDS.stopWatch, {
      accountId,
      mailboxName
    });
  }
  /**
   * Register a callback for new-mail notifications from any active watch.
   * `event.data` is a `MailNewMessagesEvent` — check `accountId`/`mailboxName`
   * before refreshing if watching more than one account.
   */
  onNewMessages(callback) {
    this.client.on(MAIL_EVENTS.NEW_MESSAGES, callback);
  }
  /** Remove a new-mail callback registered with `onNewMessages`. */
  offNewMessages(callback) {
    this.client.off(MAIL_EVENTS.NEW_MESSAGES, callback);
  }
};
init_esm_shims();
var NotificationsAPI = class {
  constructor(client) {
    this.client = client;
  }
  /** Show a notification. Returns its id so it can be dismissed later. */
  async show(opts) {
    return this.client.request(NOTIFICATION_COMMANDS.show, {
      options: opts
    });
  }
  /** Dismiss a previously shown notification (only own notifications). */
  async dismiss(id) {
    return this.client.request(NOTIFICATION_COMMANDS.dismiss, { id });
  }
  /**
   * Listen for clicks on this extension's notifications. Useful when the
   * extension is already open and wants to react in-app (e.g. router.push the
   * `path`) instead of relying on the host to focus the webview.
   *
   * Returns an unsubscribe function.
   */
  onClick(handler) {
    const wrapped = (event) => {
      const data = event.data;
      if (data) handler(data);
    };
    this.client.on(NOTIFICATION_EVENTS.CLICK, wrapped);
    return () => this.client.off(NOTIFICATION_EVENTS.CLICK, wrapped);
  }
};
init_esm_shims();
var TabAPI = class {
  constructor(client) {
    this.client = client;
  }
  /**
   * Mark the tab as wanting the user's attention (`true`), e.g. for unread
   * content, or clear the mark (`false`).
   */
  async requestAttention(active) {
    await this.client.request(EXTENSION_COMMANDS.tabAttention, { active });
  }
};
init_esm_shims();
var DialogAPI = class {
  constructor(client) {
    this.client = client;
  }
  /** Asks the user; resolves `true` for confirm, `false` for cancel or when the dialog is closed. */
  async confirm(options) {
    const answer = await this.client.request(EXTENSION_COMMANDS.dialogConfirm, { ...options }, { timeout: null });
    return answer === true;
  }
};
init_esm_shims();
init_esm_shims();
init_esm_shims();
function fail(field, message) {
  return { ok: false, field, message };
}
function typeMatches(type, value) {
  switch (type) {
    case "object":
      return typeof value === "object" && value !== null && !Array.isArray(value);
    case "array":
      return Array.isArray(value);
    case "integer":
      return Number.isInteger(value);
    case "number":
      return typeof value === "number" && Number.isFinite(value);
    case "string":
      return typeof value === "string";
    case "boolean":
      return typeof value === "boolean";
    default:
      return false;
  }
}
function validateAt(schema, value, field) {
  if (!typeMatches(schema.type, value)) return fail(field, `expected ${String(schema.type)}`);
  if (schema.enum && !schema.enum.includes(value)) {
    return fail(field, `expected one of ${schema.enum.join(", ")}`);
  }
  if (schema.type === "array" && schema.items) {
    const items = value;
    for (let i = 0; i < items.length; i++) {
      const result = validateAt(schema.items, items[i], `${field}[${i}]`);
      if (!result.ok) return result;
    }
  }
  if (schema.type === "object") {
    const record = value;
    const properties = schema.properties ?? {};
    const prefix = field === "" ? "" : `${field}.`;
    if (schema.properties) {
      for (const key of Object.keys(record)) {
        if (!Object.hasOwn(properties, key)) return fail(`${prefix}${key}`, "unknown property");
      }
    }
    for (const key of schema.required ?? []) {
      if (record[key] === void 0) return fail(`${prefix}${key}`, "required");
    }
    for (const [key, child] of Object.entries(properties)) {
      if (record[key] === void 0) continue;
      const result = validateAt(child, record[key], `${prefix}${key}`);
      if (!result.ok) return result;
    }
  }
  return { ok: true };
}
function validateInput(schema, value) {
  return validateAt(schema, value, "");
}
var PROTOCOL_VERSION = "2025-11-25";
var METHOD_NOT_FOUND = -32601;
var INVALID_PARAMS = -32602;
function textResult(text, isError) {
  return { content: [{ type: "text", text }], isError };
}
function asText(value) {
  if (typeof value === "string") return value;
  if (value === void 0) return "";
  return JSON.stringify(value);
}
function isId(value) {
  return typeof value === "string" || typeof value === "number";
}
var ToolsServer = class {
  constructor(options) {
    this.options = options;
    this.running = /* @__PURE__ */ new Map();
  }
  /**
   * Handles one JSON-RPC message from the host. A tool call runs on its own and answers when it is
   * done, so a long call never holds up `ping` or the cancellation of that very call.
   */
  async receive(message) {
    if (typeof message !== "object" || message === null) return;
    const { id, method, params } = message;
    if (typeof method !== "string") return;
    const args = typeof params === "object" && params !== null ? params : {};
    if (!isId(id)) {
      if (method === "notifications/cancelled" && isId(args.requestId)) {
        this.running.get(args.requestId)?.abort();
      }
      return;
    }
    switch (method) {
      case "initialize":
        return this.reply(id, {
          protocolVersion: PROTOCOL_VERSION,
          capabilities: { tools: {} },
          serverInfo: {
            name: this.options.manifest?.name ?? "",
            version: this.options.manifest?.version ?? ""
          }
        });
      case "ping":
        return this.reply(id, {});
      case "tools/list": {
        const tools = this.tools().map(({ name, title, description, inputSchema }) => ({
          name,
          title: title.en,
          description,
          inputSchema
        }));
        return this.reply(id, { tools });
      }
      case "tools/call":
        void this.call(id, args);
        return;
      default:
        return this.error(id, METHOD_NOT_FOUND, method);
    }
  }
  /** The declared tools that have a handler. */
  tools() {
    return (this.options.manifest?.tools ?? []).filter((tool) => this.options.handlers.has(tool.name));
  }
  async call(id, params) {
    const tool = this.tools().find((candidate) => candidate.name === params.name);
    const handler = tool && this.options.handlers.get(tool.name);
    if (!tool || !handler) return this.error(id, INVALID_PARAMS, `unknown tool ${String(params.name)}`);
    const input = params.arguments ?? {};
    const checked = validateInput(tool.inputSchema, input);
    if (!checked.ok) {
      return this.reply(id, textResult(`invalid input: ${checked.field}: ${checked.message}`, true));
    }
    const controller = new AbortController();
    this.running.set(id, controller);
    let result;
    try {
      const output = await handler(input, { signal: controller.signal });
      result = textResult(asText(output), false);
    } catch (error) {
      result = textResult(error instanceof Error ? error.message : String(error), true);
    } finally {
      this.running.delete(id);
    }
    if (!controller.signal.aborted) this.reply(id, result);
  }
  reply(id, result) {
    this.options.send({ jsonrpc: "2.0", id, result });
  }
  error(id, code, message) {
    this.options.send({ jsonrpc: "2.0", id, error: { code, message } });
  }
};
var ToolsAPI = class {
  constructor(options) {
    this.options = options;
    this.handlers = /* @__PURE__ */ new Map();
    const manifest = options.manifest;
    this.server = new ToolsServer({
      get manifest() {
        return manifest();
      },
      handlers: this.handlers,
      send: options.send
    });
  }
  /**
   * Runs `handler` when the host calls the tool `name`. The tool must be declared in the manifest.
   * Returns a function that unregisters the handler; the host then no longer sees the tool.
   */
  register(name, handler) {
    const declared = this.options.manifest()?.tools?.some((tool) => tool.name === name) ?? false;
    if (!declared) {
      throw new Error(`The tool ${name} is not declared in the manifest's tools`);
    }
    this.handlers.set(name, handler);
    return () => {
      if (this.handlers.get(name) === handler) this.handlers.delete(name);
    };
  }
  /** Handles one MCP message from the host. Called by the SDK for `haexspace:mcp` port messages. */
  receive(message) {
    return this.server.receive(message);
  }
};
init_esm_shims();
function validatePublicKey(publicKey) {
  if (!publicKey || typeof publicKey !== "string" || publicKey.trim() === "") {
    throw new HaexVaultSdkError(
      "INVALID_PUBLIC_KEY",
      "errors.invalid_public_key",
      { publicKey }
    );
  }
}
function validateExtensionName(extensionName) {
  if (!extensionName || !/^[a-z][a-z0-9-]*$/i.test(extensionName)) {
    throw new HaexVaultSdkError(
      "INVALID_EXTENSION_NAME",
      "errors.invalid_extension_name",
      { extensionName }
    );
  }
  if (extensionName.includes(TABLE_SEPARATOR)) {
    throw new HaexVaultSdkError(
      "INVALID_EXTENSION_NAME",
      "errors.extension_name_contains_separator",
      { extensionName, separator: TABLE_SEPARATOR }
    );
  }
}
function validateTableName(tableName) {
  if (!tableName || typeof tableName !== "string") {
    throw new HaexVaultSdkError(
      "INVALID_TABLE_NAME",
      "errors.table_name_empty"
    );
  }
  if (tableName.includes(TABLE_SEPARATOR)) {
    throw new HaexVaultSdkError(
      "INVALID_TABLE_NAME",
      "errors.table_name_contains_separator",
      { tableName, separator: TABLE_SEPARATOR }
    );
  }
  if (!/^[a-z][a-z0-9-_]*$/i.test(tableName)) {
    throw new HaexVaultSdkError(
      "INVALID_TABLE_NAME",
      "errors.table_name_format",
      { tableName }
    );
  }
}
function getExtensionTableName(extensionInfo, tableName) {
  if (!extensionInfo) {
    throw new HaexVaultSdkError(
      "EXTENSION_INFO_UNAVAILABLE",
      "errors.extension_info_unavailable"
    );
  }
  validateTableName(tableName);
  const { publicKey, name } = extensionInfo;
  return `"${getTableName2(publicKey, name, tableName)}"`;
}
function getDependencyTableName(publicKey, extensionName, tableName) {
  validatePublicKey(publicKey);
  validateExtensionName(extensionName);
  validateTableName(tableName);
  return `"${getTableName2(publicKey, extensionName, tableName)}"`;
}
function parseTableName(fullTableName) {
  let cleanTableName = fullTableName;
  if (cleanTableName.startsWith('"') && cleanTableName.endsWith('"')) {
    cleanTableName = cleanTableName.slice(1, -1);
  }
  const parts = cleanTableName.split(TABLE_SEPARATOR);
  if (parts.length !== 3) {
    return null;
  }
  const [publicKey, extensionName, tableName] = parts;
  if (!publicKey || !extensionName || !tableName) {
    return null;
  }
  return {
    publicKey,
    extensionName,
    tableName
  };
}
init_esm_shims();
var PORT_HANDSHAKE_TIMEOUT_MS = 1e4;
function getCurrentWebviewLabel() {
  const internals = window.__TAURI_INTERNALS__;
  return internals?.metadata?.currentWebview?.label;
}
function isInIframe() {
  return window.self !== window.top;
}
function hasTauri() {
  return typeof window.__TAURI__ !== "undefined";
}
function getTauriCore() {
  return window.__TAURI__.core;
}
function getTauriEvent() {
  return window.__TAURI__.event;
}
async function initNativeMode(ctx, log2, onEvent, onContextChange) {
  const { invoke } = getTauriCore();
  const extensionInfo = await invoke(EXTENSION_COMMANDS.getInfo);
  const context = await invoke(EXTENSION_COMMANDS.getContext);
  ctx.state.isNativeWindow = true;
  ctx.state.initialized = true;
  ctx.state.extensionInfo = extensionInfo;
  ctx.state.context = context;
  log2("HaexVault SDK initialized in native WebViewWindow mode");
  log2("Extension info:", extensionInfo);
  log2("Application context:", context);
  await setupTauriEventListeners(ctx, log2, onEvent, onContextChange);
  return { extensionInfo, context };
}
async function forwardEvent(listen, log2, onEvent, listenOptions, eventName, shape) {
  try {
    await listen(eventName, (event) => {
      if (event.payload == null) {
        log2(`Event '${eventName}' received with no payload`);
        return;
      }
      const extra = shape ? shape(event.payload) : { data: event.payload };
      onEvent({ type: eventName, timestamp: Date.now(), ...extra });
    }, listenOptions);
    log2(`Listener registered: ${eventName}`);
  } catch (error) {
    log2(`Failed to register listener '${eventName}':`, error);
  }
}
async function setupTauriEventListeners(ctx, log2, onEvent, onContextChange) {
  const { listen } = getTauriEvent();
  const webviewLabel = getCurrentWebviewLabel();
  const listenOptions = webviewLabel ? { target: webviewLabel } : void 0;
  if (!webviewLabel) {
    log2(
      "WARNING: could not read __TAURI_INTERNALS__.metadata.currentWebview.label \u2014 registering listeners without a target. Label-scoped emits from the host will not be delivered until the metadata is available."
    );
  }
  log2("Setting up Tauri event listener for:", HAEXTENSION_EVENTS.CONTEXT_CHANGED);
  try {
    await listen(HAEXTENSION_EVENTS.CONTEXT_CHANGED, (event) => {
      log2("Received Tauri event:", HAEXTENSION_EVENTS.CONTEXT_CHANGED, event);
      const payload = event.payload;
      if (payload?.context) {
        ctx.state.context = payload.context;
        log2("Updated context to:", ctx.state.context);
        onContextChange(payload.context);
        onEvent({
          type: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
          data: { context: ctx.state.context },
          timestamp: Date.now()
        });
      } else {
        log2("Event received but no context in payload:", event);
      }
    }, listenOptions);
    log2("Context change listener registered successfully");
  } catch (error) {
    log2("Failed to setup context change listener:", error);
  }
  for (const eventName of [
    HAEXTENSION_EVENTS.PERMISSION_RESOLVED,
    EXTERNAL_EVENTS.REQUEST,
    EXTERNAL_EVENTS.ACTION_REQUEST,
    NOTIFICATION_EVENTS.CLICK,
    MAIL_EVENTS.NEW_MESSAGES,
    ...Object.values(LOCALSEND_EVENTS)
  ]) {
    await forwardEvent(listen, log2, onEvent, listenOptions, eventName);
  }
  await forwardEvent(listen, log2, onEvent, listenOptions, HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED, (payload) => ({
    data: { tables: payload.tables }
  }));
  await forwardEvent(listen, log2, onEvent, listenOptions, HAEXTENSION_EVENTS.FILE_CHANGED, (payload) => {
    const { ruleId, changeType, path: path2 } = payload;
    return { ruleId, changeType, path: path2 };
  });
  await forwardEvent(listen, log2, onEvent, listenOptions, SHELL_EVENTS.OUTPUT, (payload) => {
    const { sessionId, data } = payload;
    return { sessionId, data };
  });
  await forwardEvent(listen, log2, onEvent, listenOptions, SHELL_EVENTS.EXIT, (payload) => {
    const { sessionId, exitCode } = payload;
    return { sessionId, exitCode };
  });
}
async function initIframeMode(ctx, log2, messageHandler) {
  if (!isInIframe()) {
    throw new HaexVaultSdkError("NOT_IN_IFRAME", "errors.not_in_iframe");
  }
  const port = await waitForHostPortAsync(log2);
  ctx.handlers.messageHandler = messageHandler;
  port.addEventListener("message", messageHandler);
  port.start();
  port.postMessage({ type: HAEXSPACE_MESSAGE_TYPES.PORT_READY });
  ctx.state.isNativeWindow = false;
  ctx.state.initialized = true;
  log2("HaexVault SDK initialized in iframe mode (MessagePort transport)");
  if (ctx.config.manifest) {
    ctx.state.extensionInfo = {
      publicKey: ctx.config.manifest.publicKey,
      name: ctx.config.manifest.name,
      version: ctx.config.manifest.version,
      displayName: ctx.config.manifest.displayName ?? ctx.config.manifest.name
    };
    log2("Extension info loaded from manifest:", ctx.state.extensionInfo);
  }
  sendDebugInfo(ctx.config);
  return port;
}
function waitForHostPortAsync(log2) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const cleanup = () => {
      window.removeEventListener("message", handler);
    };
    const timeoutId = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(
        new HaexVaultSdkError(
          "TIMEOUT",
          "errors.port_handshake_timeout",
          { timeout: PORT_HANDSHAKE_TIMEOUT_MS }
        )
      );
    }, PORT_HANDSHAKE_TIMEOUT_MS);
    const handler = (event) => {
      const type = event.data?.type;
      if (type !== HAEXSPACE_MESSAGE_TYPES.PORT_INIT) return;
      if (event.source !== window.parent) {
        log2("PORT_INIT from a window other than the parent \u2014 ignoring");
        return;
      }
      const port = event.ports[0];
      if (!port) {
        log2("PORT_INIT received but event.ports is empty \u2014 ignoring");
        return;
      }
      if (settled) return;
      settled = true;
      clearTimeout(timeoutId);
      cleanup();
      resolve(port);
    };
    window.addEventListener("message", handler);
  });
}
function sendDebugInfo(config) {
  if (!config.debug) return;
  if (typeof window === "undefined" || !window.parent) return;
  const debugInfo = `SDK Debug:
window.parent exists: ${!!window.parent}
window.parent === window: ${window.parent === window}
window.self === window.top: ${window.self === window.top}`;
  try {
    window.parent.postMessage({
      type: HAEXSPACE_MESSAGE_TYPES.DEBUG,
      data: debugInfo
    }, "*");
  } catch (e) {
    alert(debugInfo + `
postMessage error: ${e}`);
  }
}
init_esm_shims();
function generateRequestId(counter) {
  return `req_${counter}`;
}
function sendPostMessage(method, params, requestId, config, extensionInfo, pendingRequests, port, timeoutMs = config.timeout) {
  if (!port) {
    return Promise.reject(
      new HaexVaultSdkError(
        "EXTENSION_NOT_INITIALIZED",
        "errors.port_not_connected"
      )
    );
  }
  const request = {
    method,
    params,
    timestamp: Date.now()
  };
  return new Promise((resolve, reject) => {
    const timeout = timeoutMs === null ? void 0 : setTimeout(() => {
      pendingRequests.delete(requestId);
      reject(
        new HaexVaultSdkError("TIMEOUT", "errors.timeout", {
          timeout: timeoutMs
        })
      );
    }, timeoutMs);
    pendingRequests.set(requestId, { resolve, reject, timeout });
    if (config.debug) {
      console.log("[SDK Debug] ========== Sending Request ==========");
      console.log("[SDK Debug] Request ID:", requestId);
      console.log("[SDK Debug] Method:", request.method);
      console.log("[SDK Debug] Params:", request.params);
      console.log("[SDK Debug] Transport: MessagePort");
      console.log("[SDK Debug] Extension info:", extensionInfo);
      console.log("[SDK Debug] ========================================");
    }
    port.postMessage({ id: requestId, ...request });
  });
}
async function sendInvoke(method, params, config, _log) {
  const { invoke } = window.__TAURI__.core;
  if (config.debug) {
    console.log("[SDK Debug] ========== Invoke Request ==========");
    console.log("[SDK Debug] Command:", method);
    console.log("[SDK Debug] Params:", params);
    console.log("[SDK Debug] =======================================");
  }
  return invoke(method, params);
}
init_esm_shims();
function createMessageHandler(config, pendingRequests, extensionInfo, onEvent) {
  return (event) => {
    if (config.debug) {
      console.log("[SDK Debug] ========== Message Received ==========");
      console.log("[SDK Debug] Event data:", event.data);
      console.log("[SDK Debug] Extension info loaded:", !!extensionInfo());
      console.log(
        "[SDK Debug] Pending requests count:",
        pendingRequests.size
      );
    }
    const data = event.data;
    if ("id" in data && pendingRequests.has(data.id)) {
      if (config.debug) {
        console.log("[SDK Debug] \u2705 Found pending request for ID:", data.id);
      }
      const pending = pendingRequests.get(data.id);
      clearTimeout(pending.timeout);
      pendingRequests.delete(data.id);
      if (data.error) {
        if (config.debug) {
          console.error("[SDK Debug] \u274C Request failed:", data.error);
        }
        pending.reject(data.error);
      } else {
        if (config.debug) {
          console.log("[SDK Debug] \u2705 Request succeeded:", data.result);
        }
        pending.resolve(data.result);
      }
      return;
    }
    if ("id" in data && !pendingRequests.has(data.id)) {
      if (config.debug) {
        console.warn(
          "[SDK Debug] \u26A0\uFE0F Received response for unknown request ID:",
          data.id
        );
        console.warn(
          "[SDK Debug] Known IDs:",
          Array.from(pendingRequests.keys())
        );
      }
    }
    if ("type" in data && data.type) {
      if (config.debug) {
        console.log("[SDK Debug] Event received:", data.type);
      }
      onEvent(data);
    }
    if (config.debug) {
      console.log("[SDK Debug] ========== End Message ==========");
    }
  };
}
function processEvent(event, log2, eventListeners, onContextChanged, onExternalRequest, onActionRequest, onMcpMessage) {
  if (event.type === HAEXSPACE_MESSAGE_TYPES.MCP) {
    onMcpMessage?.(event.message);
    return;
  }
  if (event.type === HAEXTENSION_EVENTS.CONTEXT_CHANGED) {
    const contextEvent = event;
    onContextChanged(contextEvent.data.context);
    log2("Context updated:", contextEvent.data.context);
  }
  if (event.type === EXTERNAL_EVENTS.REQUEST) {
    const externalEvent = event;
    onExternalRequest(externalEvent);
    return;
  }
  if (event.type === EXTERNAL_EVENTS.ACTION_REQUEST) {
    const actionEvent = event;
    if (onActionRequest) {
      onActionRequest(actionEvent);
    }
    return;
  }
  emitEvent(event, log2, eventListeners);
}
function emitEvent(event, log2, eventListeners) {
  log2("emitEvent called with:", event.type, event);
  log2("Registered event types:", Array.from(eventListeners.keys()));
  const listeners = eventListeners.get(event.type);
  log2("Listeners for", event.type, ":", listeners?.size ?? 0);
  if (listeners) {
    listeners.forEach((callback) => callback(event));
  }
}
function addEventListener(eventType, callback, eventListeners) {
  if (!eventListeners.has(eventType)) {
    eventListeners.set(eventType, /* @__PURE__ */ new Set());
  }
  eventListeners.get(eventType).add(callback);
}
function removeEventListener(eventType, callback, eventListeners) {
  const listeners = eventListeners.get(eventType);
  if (listeners) {
    listeners.delete(callback);
  }
}
function notifySubscribers(subscribers) {
  subscribers.forEach((callback) => callback());
}
init_esm_shims();
function createDrizzleInstance(schema, extensionInfo, request, log2) {
  if (!extensionInfo) {
    throw new HaexVaultSdkError(
      "EXTENSION_INFO_UNAVAILABLE",
      "errors.client_not_ready"
    );
  }
  return drizzle(
    async (sql2, params, method) => {
      try {
        if (method === "run" || method === "all") {
          const result2 = await request(
            DATABASE_COMMANDS.execute,
            {
              sql: sql2,
              params
            }
          );
          if (method === "all") {
            return { rows: result2.rows || [] };
          }
          if (result2.rows && Array.isArray(result2.rows) && result2.rows.length > 0) {
            return { rows: result2.rows };
          }
          return { rows: [] };
        }
        const result = await request(DATABASE_COMMANDS.query, {
          sql: sql2,
          params
        });
        const rows = result.rows;
        if (method === "get") {
          return { rows: rows.length > 0 ? [rows[0]] : [] };
        }
        return { rows };
      } catch (error) {
        log2("Database operation failed:", error);
        throw error;
      }
    },
    {
      schema,
      logger: false
    }
  );
}
async function queryRaw(sql2, params, request, debug) {
  const result = await request(
    DATABASE_COMMANDS.query,
    { sql: sql2, params }
  );
  if (debug) {
    console.log("[SDK query()] Raw result:", JSON.stringify(result, null, 2));
  }
  return result.rows;
}
async function executeRaw(sql2, params, request) {
  const result = await request(
    DATABASE_COMMANDS.execute,
    { sql: sql2, params }
  );
  return {
    rowsAffected: result.rowsAffected,
    lastInsertId: result.lastInsertId
  };
}
init_esm_shims();
function registerExternalHandler(action, handler, handlers, log2) {
  handlers.set(action, handler);
  log2(`[ExternalRequest] Registered handler for action: ${action}`);
  return () => {
    handlers.delete(action);
    log2(`[ExternalRequest] Unregistered handler for action: ${action}`);
  };
}
async function handleExternalRequest(request, handlers, respond, log2) {
  log2("handleExternalRequest called");
  log2("Request:", request);
  log2("Available handlers:", Array.from(handlers.keys()));
  log2(`[ExternalRequest] Received request: ${request.action} from ${request.publicKey.substring(0, 20)}...`);
  const handler = handlers.get(request.action);
  if (!handler) {
    log2(`[ExternalRequest] No handler for action: ${request.action}`);
    await respond({
      requestId: request.requestId,
      success: false,
      error: `No handler registered for action: ${request.action}`
    });
    return;
  }
  try {
    const response = await handler(request);
    await respond(response);
    log2(`[ExternalRequest] Response sent for: ${request.action}`);
  } catch (error) {
    log2(`[ExternalRequest] Handler error:`, error);
    await respond({
      requestId: request.requestId,
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}
async function respondToExternalRequest(response, request) {
  await request(EXTERNAL_BRIDGE_COMMANDS.respond, response);
}
init_esm_shims();
var AI_COMMANDS = {
  /** Respond to an AI action request */
  actionRespond: "ai_action_respond"
};
init_esm_shims();
var PERMISSION_DECISION_TIMEOUT_MS = 5 * 60 * 1e3;
var MAX_PERMISSION_RETRIES = 3;
function permissionKey(resourceType, action, target) {
  return `${resourceType}:${action}:${target}`;
}
var PermissionWaiterRegistry = class {
  constructor() {
    this.waiters = /* @__PURE__ */ new Map();
  }
  /** Wait for a decision on `key`; resolves "timeout" if none arrives in time. */
  wait(key, timeoutMs = PERMISSION_DECISION_TIMEOUT_MS) {
    return new Promise((resolve) => {
      const set = this.waiters.get(key) ?? /* @__PURE__ */ new Set();
      this.waiters.set(key, set);
      const settle = (outcome) => {
        if (!set.has(callback)) return;
        set.delete(callback);
        if (set.size === 0) this.waiters.delete(key);
        clearTimeout(timer);
        resolve(outcome);
      };
      const callback = (outcome) => settle(outcome);
      const timer = setTimeout(() => settle("timeout"), timeoutMs);
      set.add(callback);
    });
  }
  /** Resolve everyone waiting on `key` with the user's decision. */
  resolve(key, decision) {
    const set = this.waiters.get(key);
    if (!set) return;
    for (const callback of [...set]) callback(decision);
  }
};
function toDeniedError(error) {
  return {
    ...error,
    code: 1002
    /* DENIED */
  };
}
async function withPermissionRetry(send, registry, log2, timeoutMs = PERMISSION_DECISION_TIMEOUT_MS) {
  for (let attempt = 0; ; attempt++) {
    try {
      return await send();
    } catch (error) {
      if (!isPermissionPromptError(error) || attempt >= MAX_PERMISSION_RETRIES) {
        throw error;
      }
      const key = permissionKey(error.resourceType, error.action, error.target);
      log2(`Permission prompt required for ${key} \u2014 waiting for user decision`);
      const outcome = await registry.wait(key, timeoutMs);
      if (outcome === "granted") {
        log2(`Permission ${key} granted \u2014 retrying request`);
        continue;
      }
      if (outcome === "denied") {
        log2(`Permission ${key} denied`);
        throw toDeniedError(error);
      }
      log2(`Permission ${key} prompt timed out`);
      throw error;
    }
  }
}
var HaexVaultSdk = class {
  constructor(config = {}) {
    this.initialized = false;
    this.isNativeWindow = false;
    this.requestCounter = 0;
    this._extensionInfo = null;
    this._context = null;
    this._setupCompleted = false;
    this.pendingRequests = /* @__PURE__ */ new Map();
    this.eventListeners = /* @__PURE__ */ new Map();
    this.externalRequestHandlers = /* @__PURE__ */ new Map();
    this.reactiveSubscribers = /* @__PURE__ */ new Set();
    this.permissionWaiters = new PermissionWaiterRegistry();
    this.messageHandler = null;
    this.hostPort = null;
    this.setupPromise = null;
    this.setupHook = null;
    this.orm = null;
    this.actions = {
      register: (action, handler) => {
        return this.onExternalRequest(action, handler);
      }
    };
    this.config = {
      debug: config.debug ?? false,
      timeout: config.timeout ?? DEFAULT_TIMEOUT,
      manifest: config.manifest
    };
    this.storage = new StorageAPI(this);
    this.database = new DatabaseAPI(this);
    this.filesystem = new FilesystemAPI(this);
    this.web = new WebAPI(this);
    this.permissions = new PermissionsAPI(this);
    this.remoteStorage = new RemoteStorageAPI(this);
    this.encryptedFolders = new EncryptedFoldersAPI(this);
    this.localsend = new LocalSendAPI(this);
    this.spaces = new SpacesAPI(this);
    this.shell = new ShellAPI(this);
    this.passwords = new PasswordsAPI(this);
    this.mail = new MailAPI(this);
    this.notifications = new NotificationsAPI(this);
    this.tab = new TabAPI(this);
    this.dialog = new DialogAPI(this);
    this.tools = new ToolsAPI({
      manifest: () => this.config.manifest,
      send: (message) => this.sendMcpMessage(message)
    });
    installConsoleForwarding(this.config.debug);
    this.on(HAEXTENSION_EVENTS.PERMISSION_RESOLVED, (event) => {
      const data = event.data;
      if (!data) return;
      this.permissionWaiters.resolve(
        permissionKey(data.resourceType, data.action, data.target),
        data.decision === "denied" ? "denied" : "granted"
      );
    });
    this.readyPromise = new Promise((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    this.init().catch((error) => {
      this.log("Init failed:", error);
      this.rejectReady(error);
    });
  }
  // ==========================================================================
  // Lifecycle
  // ==========================================================================
  async ready() {
    return this.readyPromise;
  }
  get setupCompleted() {
    return this._setupCompleted;
  }
  onSetup(setupFn) {
    if (this.setupHook) {
      throw new Error("Setup hook already registered");
    }
    this.setupHook = setupFn;
  }
  async setupComplete() {
    await this.readyPromise;
    if (!this.setupHook || this.setupCompleted) {
      return;
    }
    if (!this.setupPromise) {
      this.setupPromise = this.runSetupAsync();
    }
    return this.setupPromise;
  }
  destroy() {
    if (this.messageHandler && this.hostPort) {
      this.hostPort.removeEventListener("message", this.messageHandler);
    }
    if (this.hostPort) {
      this.hostPort.close();
      this.hostPort = null;
    }
    this.messageHandler = null;
    this.pendingRequests.forEach(({ timeout }) => clearTimeout(timeout));
    this.pendingRequests.clear();
    this.eventListeners.clear();
    this.initialized = false;
    this.log("HaexVault SDK destroyed");
  }
  // ==========================================================================
  // Properties
  // ==========================================================================
  get extensionInfo() {
    return this._extensionInfo;
  }
  get context() {
    return this._context;
  }
  // ==========================================================================
  // Subscriptions
  // ==========================================================================
  subscribe(callback) {
    this.reactiveSubscribers.add(callback);
    return () => {
      this.reactiveSubscribers.delete(callback);
    };
  }
  // ==========================================================================
  // Table Name Utilities
  // ==========================================================================
  getTableName(tableName) {
    return getExtensionTableName(this._extensionInfo, tableName);
  }
  getDependencyTableName(publicKey, extensionName, tableName) {
    return getDependencyTableName(publicKey, extensionName, tableName);
  }
  parseTableName(fullTableName) {
    return parseTableName(fullTableName);
  }
  // ==========================================================================
  // Database
  // ==========================================================================
  initializeDatabase(schema) {
    const db = createDrizzleInstance(schema, this._extensionInfo, this.request.bind(this), this.log.bind(this));
    this.orm = db;
    return db;
  }
  async query(sql2, params = []) {
    return queryRaw(sql2, params, this.request.bind(this), this.config.debug);
  }
  async select(sql2, params = []) {
    return this.query(sql2, params);
  }
  async execute(sql2, params = []) {
    return executeRaw(sql2, params, this.request.bind(this));
  }
  async registerMigrationsAsync(extensionVersion, migrations) {
    return this.database.registerMigrationsAsync(extensionVersion, migrations);
  }
  // ==========================================================================
  // Dependencies
  // ==========================================================================
  async getDependencies() {
    return this.request("extensions.getDependencies");
  }
  // ==========================================================================
  // Permissions
  // ==========================================================================
  async requestDatabasePermission(request) {
    return this.request("permissions.database.request", {
      resource: request.resource,
      operation: request.operation,
      reason: request.reason
    });
  }
  async checkDatabasePermission(resource, operation) {
    const response = await this.request("permissions.database.check", { resource, operation });
    return response.status === "granted";
  }
  // ==========================================================================
  // Search
  // ==========================================================================
  async respondToSearch(requestId, results) {
    await this.request("search.respond", { requestId, results });
  }
  // ==========================================================================
  // External Requests
  // ==========================================================================
  onExternalRequest(action, handler) {
    return registerExternalHandler(action, handler, this.externalRequestHandlers, this.log.bind(this));
  }
  async respondToExternalRequest(response) {
    await respondToExternalRequest(response, this.request.bind(this));
  }
  // ==========================================================================
  // Events
  // ==========================================================================
  on(eventType, callback) {
    addEventListener(eventType, callback, this.eventListeners);
  }
  off(eventType, callback) {
    removeEventListener(eventType, callback, this.eventListeners);
  }
  // ==========================================================================
  // Communication
  // ==========================================================================
  async request(method, params, options) {
    const resolvedParams = params ?? {};
    const send = () => {
      if (this.isNativeWindow && hasTauri()) {
        const paramsWithCredentials = {
          ...resolvedParams,
          publicKey: this._extensionInfo?.publicKey,
          name: this._extensionInfo?.name
        };
        return sendInvoke(method, paramsWithCredentials, this.config, this.log.bind(this));
      }
      const requestId = generateRequestId(++this.requestCounter);
      return sendPostMessage(
        method,
        resolvedParams,
        requestId,
        this.config,
        this._extensionInfo,
        this.pendingRequests,
        this.hostPort,
        options?.timeout
      );
    };
    return withPermissionRetry(send, this.permissionWaiters, this.log.bind(this));
  }
  // ==========================================================================
  // Private: Initialization
  // ==========================================================================
  async init() {
    if (this.initialized) return;
    if (!isInIframe() && hasTauri()) {
      try {
        await this.initNative();
        return;
      } catch (error) {
        this.log("Tauri commands failed, falling back to iframe mode", error);
      }
    }
    await this.initIframe();
  }
  async initNative() {
    const { extensionInfo, context } = await initNativeMode(
      {
        config: this.config,
        state: {
          initialized: this.initialized,
          isNativeWindow: this.isNativeWindow,
          requestCounter: this.requestCounter,
          setupCompleted: this._setupCompleted,
          extensionInfo: this._extensionInfo,
          context: this._context,
          orm: this.orm
        },
        collections: {
          pendingRequests: this.pendingRequests,
          eventListeners: this.eventListeners,
          externalRequestHandlers: this.externalRequestHandlers,
          reactiveSubscribers: this.reactiveSubscribers
        },
        promises: {
          readyPromise: this.readyPromise,
          resolveReady: this.resolveReady,
          setupPromise: this.setupPromise,
          setupHook: this.setupHook
        },
        handlers: {
          messageHandler: this.messageHandler
        }
      },
      this.log.bind(this),
      this.handleEvent.bind(this),
      (ctx) => {
        this._context = ctx;
        this.notifySubscribersInternal();
      }
    );
    this._extensionInfo = extensionInfo;
    this._context = context;
    this.isNativeWindow = true;
    this.initialized = true;
    this.notifySubscribersInternal();
    this.resolveReady();
  }
  async initIframe() {
    this.messageHandler = createMessageHandler(
      this.config,
      this.pendingRequests,
      () => this._extensionInfo,
      this.handleEvent.bind(this)
    );
    const port = await initIframeMode(
      {
        config: this.config,
        state: {
          initialized: this.initialized,
          isNativeWindow: this.isNativeWindow,
          requestCounter: this.requestCounter,
          setupCompleted: this._setupCompleted,
          extensionInfo: this._extensionInfo,
          context: this._context,
          orm: this.orm
        },
        collections: {
          pendingRequests: this.pendingRequests,
          eventListeners: this.eventListeners,
          externalRequestHandlers: this.externalRequestHandlers,
          reactiveSubscribers: this.reactiveSubscribers
        },
        promises: {
          readyPromise: this.readyPromise,
          resolveReady: this.resolveReady,
          setupPromise: this.setupPromise,
          setupHook: this.setupHook
        },
        handlers: {
          messageHandler: this.messageHandler
        }
      },
      this.log.bind(this),
      this.messageHandler
    );
    this.hostPort = port;
    if (this.config.manifest) {
      this._extensionInfo = {
        publicKey: this.config.manifest.publicKey,
        name: this.config.manifest.name,
        version: this.config.manifest.version,
        displayName: this.config.manifest.displayName ?? this.config.manifest.name
      };
      this.notifySubscribersInternal();
    }
    const context = await this.request(EXTENSION_COMMANDS.getContext);
    this.log("Application context received:", context);
    this._context = context;
    this.isNativeWindow = false;
    this.initialized = true;
    this.notifySubscribersInternal();
    this.resolveReady();
  }
  // ==========================================================================
  // Private: Event Handling
  // ==========================================================================
  handleEvent(event) {
    processEvent(
      event,
      this.log.bind(this),
      this.eventListeners,
      (ctx) => {
        this._context = ctx;
        this.notifySubscribersInternal();
      },
      (extEvent) => this.handleExternalRequestInternal(extEvent.data),
      (actionEvent) => this.handleActionRequestInternal(actionEvent.data),
      (message) => void this.tools.receive(message)
    );
  }
  /** Sends one MCP message of the tool connection to the host on the port. */
  sendMcpMessage(message) {
    if (!this.hostPort) {
      this.log("Dropped an MCP message: no host port yet");
      return;
    }
    this.hostPort.postMessage({ type: HAEXSPACE_MESSAGE_TYPES.MCP, message });
  }
  async handleExternalRequestInternal(request) {
    await handleExternalRequest(request, this.externalRequestHandlers, this.respondToExternalRequest.bind(this), this.log.bind(this));
  }
  async handleActionRequestInternal(request) {
    await handleExternalRequest(
      request,
      this.externalRequestHandlers,
      async (response) => {
        await this.request(AI_COMMANDS.actionRespond, response);
      },
      this.log.bind(this)
    );
  }
  // ==========================================================================
  // Private: Setup
  // ==========================================================================
  async runSetupAsync() {
    if (!this.setupHook) return;
    try {
      this.log("[HaexVault] Running setup hook...");
      await this.setupHook();
      this._setupCompleted = true;
      this.log("[HaexVault] Setup completed successfully");
      this.notifySubscribersInternal();
    } catch (error) {
      this.log("[HaexVault] Setup failed:", error);
      throw error;
    }
  }
  // ==========================================================================
  // Private: Utilities
  // ==========================================================================
  notifySubscribersInternal() {
    notifySubscribers(this.reactiveSubscribers);
  }
  log(...args) {
    if (this.config.debug) {
      console.log("[HaexVault SDK]", ...args);
    }
  }
};
init_esm_shims();
init_esm_shims();
init_vaultKey();
init_userKeypair();
init_esm_shims();
init_userKeypair();
init_vaultKey();
var ED25519_MULTICODEC_PREFIX = new Uint8Array([237, 1]);
init_esm_shims();
init_userKeypair();
init_vaultKey();
init_esm_shims();
init_userKeypair();
init_esm_shims();
init_userKeypair();
init_vaultKey();
init_esm_shims();
init_vaultKey();
function createHaexVaultSdk(config = {}) {
  return new HaexVaultSdk(config);
}

// manifest.json
var manifest_default = {
  name: "agent-tools",
  version: "1.0.0",
  displayName: "Test-Erweiterung",
  migrationsDir: "db",
  permissions: {
    notifications: [{ target: "*", operation: "show" }]
  },
  tools: [
    {
      name: "count_entries",
      title: { de: "Eintr\xE4ge z\xE4hlen", en: "Count entries" },
      description: "Counts the entries of the test extension.",
      inputSchema: { type: "object", properties: {} },
      effect: "read",
      examples: {
        de: ["Wie viele Eintr\xE4ge habe ich?", "Z\xE4hle meine Eintr\xE4ge"],
        en: ["How many entries do I have?", "Count my entries"]
      }
    },
    {
      name: "add_entry",
      title: { de: "Eintrag hinzuf\xFCgen", en: "Add entry" },
      description: "Adds an entry with the given text to the test extension and shows a notification.",
      inputSchema: {
        type: "object",
        properties: {
          text: { type: "string", description: "The text of the entry" }
        },
        required: ["text"]
      },
      effect: "change",
      examples: {
        de: ["F\xFCge einen Eintrag Milch hinzu", "Trag Brot als Eintrag ein"],
        en: ["Add an entry milk", "Add bread as an entry"]
      }
    },
    {
      name: "slow_echo",
      title: { de: "Langsames Echo", en: "Slow echo" },
      description: "Echoes the text after the given number of seconds; for time limits and cancelling.",
      inputSchema: {
        type: "object",
        properties: {
          text: { type: "string" },
          seconds: {
            type: "integer",
            description: "How long to wait, default 30"
          }
        },
        required: ["text"]
      },
      effect: "read",
      examples: {
        de: ["Gib ein langsames Echo von hallo"],
        en: ["Give a slow echo of hello"]
      }
    },
    {
      name: "optional_note",
      title: { de: "Optionale Notiz", en: "Optional note" },
      description: "Outside holzi's schema subset (a union type); installed but never offered.",
      inputSchema: {
        type: "object",
        properties: { note: { type: ["string", "null"] } }
      },
      effect: "read",
      examples: {
        de: ["Speichere eine optionale Notiz"],
        en: ["Save an optional note"]
      }
    }
  ]
};

// main.js
var sdk = createHaexVaultSdk({ manifest: manifest_default });
function log(text) {
  const item = document.createElement("li");
  item.textContent = `${(/* @__PURE__ */ new Date()).toLocaleTimeString()} ${text}`;
  document.getElementById("log").prepend(item);
  console.info("[agent-tools]", text);
}
async function table() {
  await sdk.ready();
  const info = sdk.extensionInfo;
  return getTableName2(info.publicKey, info.name, "entries");
}
async function countAsync() {
  const count = await sdk.database.count(await table());
  document.getElementById("count").textContent = String(count);
  return count;
}
sdk.tools.register("count_entries", async () => {
  const count = await countAsync();
  log(`count_entries: ${count}`);
  return { entries: count };
});
sdk.tools.register("add_entry", async (input) => {
  const text = String(input.text);
  await sdk.database.insert(await table(), { id: crypto.randomUUID(), text });
  const count = await countAsync();
  log(`add_entry: ${text}`);
  await sdk.notifications.show({ title: "Eintrag hinzugef\xFCgt", body: text });
  return { added: text, entries: count };
});
sdk.tools.register("slow_echo", (input, { signal }) => {
  const seconds = typeof input.seconds === "number" ? input.seconds : 30;
  log(`slow_echo started: ${input.text}`);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => resolve(String(input.text)), seconds * 1e3);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      log("slow_echo cancelled");
      reject(new Error("cancelled"));
    });
  });
});
countAsync().catch((error) => log(`count failed: ${error}`));
