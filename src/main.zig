const std = @import("std");
const fs = std.fs;
const json = std.json;
const ascii = std.ascii;
const mem = std.mem;
const Allocator = std.mem.Allocator;

const CLIArgs = struct {
    locales_dir: []const u8,
    output_dir: []const u8,
};

pub const Token = union(enum) {
    Text: []const u8,
    Param: []const u8,
    DoubleParam: []const u8,
    ICUParam: struct {
        key: []const u8,
        type: []const u8,
        options: std.StringHashMap([]const u8),
    },
};

pub const Tokenizer = struct {
    input: []const u8,
    index: usize,
    tokens: std.ArrayList(Token),
    allocator: Allocator,

    pub fn init(allocator: Allocator, input: []const u8) Tokenizer {
        return .{
            .input = input,
            .index = 0,
            .tokens = std.ArrayList(Token).init(allocator),
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *Tokenizer) void {
        for (self.tokens.items) |*token| {
            switch (token.*) {
                .Text, .Param, .DoubleParam => |text| self.allocator.free(text),
                .ICUParam => |*icu| {
                    self.allocator.free(icu.key);
                    var it = icu.options.iterator();
                    while (it.next()) |entry| {
                        self.allocator.free(entry.key_ptr.*);
                        self.allocator.free(entry.value_ptr.*);
                    }
                    icu.options.deinit();
                },
            }
        }
        self.tokens.deinit();
    }

    pub fn tokenize(self: *Tokenizer) !void {
        var text_start: usize = 0;
        while (self.index < self.input.len) {
            switch (self.input[self.index]) {
                '{' => {
                    try self.addTextToken(text_start, self.index);
                    try self.parseSingleParam();
                    text_start = self.index;
                },
                else => {
                    self.index += 1;
                },
            }
        }
        try self.addTextToken(text_start, self.input.len);

        std.debug.print("Tokenization complete. Total tokens: {}\n", .{self.tokens.items.len});
        for (self.tokens.items, 0..) |token, i| {
            switch (token) {
                .Text => |text| std.debug.print("Token {}: Text: {s}\n", .{ i, text }),
                .Param => |param| std.debug.print("Token {}: Param: {s}\n", .{ i, param }),
                .DoubleParam => |param| std.debug.print("Token {}: DoubleParam: {s}\n", .{ i, param }),
                .ICUParam => |icu| std.debug.print("Token {}: ICUParam: key={s}, type={s}\n", .{ i, icu.key, icu.type }),
            }
        }
    }

    fn addTextToken(self: *Tokenizer, start: usize, end: usize) !void {
        if (start < end) {
            const text = try self.allocator.dupe(u8, self.input[start..end]);
            try self.tokens.append(.{ .Text = text });
        }
    }

    fn parseSingleParam(self: *Tokenizer) !void {
        const start = self.index + 1;
        var depth: usize = 1;
        while (self.index < self.input.len and depth > 0) {
            self.index += 1;
            if (self.input[self.index] == '{') {
                depth += 1;
            } else if (self.input[self.index] == '}') {
                depth -= 1;
            }
        }
        if (self.index < self.input.len) {
            var param = try self.allocator.dupe(u8, self.input[start..self.index]);
            defer self.allocator.free(param);

            // Strip extra braces
            while (param.len > 0 and param[0] == '{') {
                param = param[1..];
            }
            while (param.len > 0 and param[param.len - 1] == '}') {
                param = param[0 .. param.len - 1];
            }

            const trimmed_param = mem.trim(u8, param, " ");
            const cleaned_param = try self.allocator.dupe(u8, trimmed_param);
            errdefer self.allocator.free(cleaned_param);

            if (mem.indexOf(u8, cleaned_param, ",") != null) {
                try self.parseICUParam(cleaned_param);
                self.allocator.free(cleaned_param);
            } else {
                try self.tokens.append(.{ .Param = cleaned_param });
            }
            self.index += 1;
        }
    }

    fn parseICUParam(self: *Tokenizer, param: []const u8) !void {
        std.debug.print("Parsing ICU param: {s}\n", .{param});
        var it = mem.split(u8, param, ",");
        const key_and_type = it.next() orelse return error.InvalidICUParam;
        var key_type_split = mem.split(u8, mem.trim(u8, key_and_type, " "), " ");
        const param_key = key_type_split.next() orelse return error.InvalidICUParam;
        var param_type = key_type_split.next() orelse "string";

        // Identify ICU message format
        if (mem.eql(u8, param_type, "select") or mem.eql(u8, param_type, "plural") or mem.eql(u8, param_type, "selectordinal")) {
            // This is an ICU message format
        } else {
            // If it's not a recognized ICU format, treat it as a regular parameter
            param_type = "string";
        }

        std.debug.print("ICU param key: {s}, type: {s}\n", .{ param_key, param_type });

        var options = std.StringHashMap([]const u8).init(self.allocator);
        errdefer options.deinit();

        while (it.next()) |option| {
            const trimmed_option = mem.trim(u8, option, " ");
            if (mem.indexOf(u8, trimmed_option, "{")) |open_brace| {
                if (mem.lastIndexOf(u8, trimmed_option, "}")) |close_brace| {
                    const option_key = mem.trim(u8, trimmed_option[0..open_brace], " ");
                    const option_value = mem.trim(u8, trimmed_option[open_brace + 1 .. close_brace], " ");
                    try options.put(try self.allocator.dupe(u8, option_key), try self.allocator.dupe(u8, option_value));
                    std.debug.print("ICU option: {s} = {s}\n", .{ option_key, option_value });
                }
            }
        }

        try self.tokens.append(.{ .ICUParam = .{
            .key = try self.allocator.dupe(u8, param_key),
            .type = try self.allocator.dupe(u8, param_type),
            .options = options,
        } });
    }
};

fn toPascalCase(allocator: Allocator, str: []const u8) ![]u8 {
    var result = try allocator.alloc(u8, str.len);
    errdefer allocator.free(result);

    var capitalize = true;
    var j: usize = 0;
    for (str) |c| {
        if (c == '_' or c == '-') {
            capitalize = true;
            continue;
        }
        if (capitalize) {
            result[j] = ascii.toUpper(c);
            capitalize = false;
        } else {
            result[j] = ascii.toLower(c);
        }
        j += 1;
    }
    return allocator.realloc(result, j);
}

fn processDirectory(allocator: Allocator, locales_dir: []const u8, output_dir: []const u8) !void {
    var dir = try fs.openDirAbsolute(locales_dir, .{ .iterate = true });
    defer dir.close();

    var walker = try dir.walk(allocator);
    defer walker.deinit();

    while (try walker.next()) |entry| {
        if (entry.kind != .file or !mem.endsWith(u8, entry.path, ".json")) {
            continue;
        }

        const project_name = fs.path.dirname(entry.path) orelse continue;
        const file_path = try fs.path.join(allocator, &[_][]const u8{ locales_dir, entry.path });
        defer allocator.free(file_path);

        const max_size = 1024 * 1024 * 100;
        const file = try fs.openFileAbsolute(file_path, .{});
        defer file.close();
        const content = try file.readToEndAlloc(allocator, max_size);
        defer allocator.free(content);

        var parsed = try json.parseFromSlice(json.Value, allocator, content, .{});
        defer parsed.deinit();

        const ts_dict = try generateTypescriptDict(allocator, parsed.value, project_name);
        defer allocator.free(ts_dict);

        try writeTypescriptFile(allocator, ts_dict, project_name, output_dir);
    }
}

fn generateTypescriptDict(allocator: Allocator, translations: json.Value, project_name: []const u8) ![]u8 {
    std.debug.print("Generating TypeScript dict for project: {s}\n", .{project_name});
    const pascal_project_name = try toPascalCase(allocator, project_name);
    defer allocator.free(pascal_project_name);

    var ts_entries = std.ArrayList(u8).init(allocator);
    defer ts_entries.deinit();

    const root = translations.object;
    var it = root.iterator();
    while (it.next()) |entry| {
        const key = entry.key_ptr.*;
        const value = entry.value_ptr.*;
        if (value != .string) continue;

        var tokenizer = Tokenizer.init(allocator, value.string);
        defer tokenizer.deinit();
        try tokenizer.tokenize();

        var params = std.StringHashMap([]const u8).init(allocator);
        defer params.deinit();

        for (tokenizer.tokens.items) |token| {
            switch (token) {
                .Param, .DoubleParam => |param| {
                    const escaped_param = try escapeParamName(param, allocator);
                    try params.put(escaped_param, "string");
                    std.debug.print("Added Param: {s}: string\n", .{escaped_param});
                },
                .ICUParam => |icu| {
                    const param_type = getICUParamType(icu.type);
                    const escaped_key = try escapeParamName(icu.key, allocator);
                    try params.put(escaped_key, param_type);
                    std.debug.print("Added ICUParam: {s}: {s}\n", .{ escaped_key, param_type });
                },
                .Text => {},
            }
        }

        if (params.count() > 0) {
            try ts_entries.writer().print("    '{s}': (params: {{", .{key});
            var param_it = params.iterator();
            var first = true;
            while (param_it.next()) |param| {
                if (!first) try ts_entries.writer().print(", ", .{});
                try ts_entries.writer().print("{s}: {s}", .{ param.key_ptr.*, param.value_ptr.* });
                first = false;
            }
            try ts_entries.writer().print("}}) => string;\n", .{});
        } else {
            try ts_entries.writer().print("    '{s}': string;\n", .{key});
        }
    }

    return try std.fmt.allocPrint(allocator,
        \\export type {s}Dict = {{
        \\{s}}};
        \\
    , .{ pascal_project_name, ts_entries.items });
}

fn getICUParamType(icu_type: []const u8) []const u8 {
    if (mem.eql(u8, icu_type, "plural") or mem.eql(u8, icu_type, "selectordinal")) {
        return "number";
    } else if (mem.eql(u8, icu_type, "select")) {
        return "string";
    } else {
        return "string";
    }
}

fn escapeParamName(param: []const u8, allocator: Allocator) ![]const u8 {
    if (param.len > 0 and ascii.isDigit(param[0])) {
        return std.fmt.allocPrint(allocator, "__{s}", .{param});
    }
    return param;
}

fn writeTypescriptFile(allocator: Allocator, ts_content: []const u8, project_name: []const u8, output_dir: []const u8) !void {
    const pascal_project_name = try toPascalCase(allocator, project_name);
    defer allocator.free(pascal_project_name);

    const file_name = try std.fmt.allocPrint(allocator, "{s}Translations.ts", .{pascal_project_name});
    defer allocator.free(file_name);

    const file_path = try fs.path.join(allocator, &[_][]const u8{ output_dir, file_name });
    defer allocator.free(file_path);

    try fs.cwd().makePath(fs.path.dirname(file_path).?);

    const file = try fs.createFileAbsolute(file_path, .{});
    defer file.close();

    try file.writeAll(ts_content);
}

pub fn main() !void {
    const start_time = std.time.milliTimestamp();
    var gpa = std.heap.GeneralPurposeAllocator(.{ .enable_memory_limit = true }){};
    defer {
        const leaked = gpa.deinit();
        if (leaked == .leak) {
            std.debug.print("Memory leak detected!\n", .{});
        }
    }
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len != 3) {
        std.debug.print("Usage: {s} <locales_dir> <output_dir>\n", .{args[0]});
        return error.InvalidArguments;
    }

    const cli_args = CLIArgs{
        .locales_dir = args[1],
        .output_dir = args[2],
    };

    try processDirectory(allocator, cli_args.locales_dir, cli_args.output_dir);
    std.debug.print("TypeScript definitions generated successfully.\n", .{});
    const end_time = std.time.milliTimestamp();
    const elapsed_ms = end_time - start_time;
    std.debug.print("Execution time: {} ms\n", .{elapsed_ms});
    const total_bytes = gpa.total_requested_bytes;
    std.debug.print("Total memory used: {} bytes\n", .{total_bytes});
}
