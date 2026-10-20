#include <windows.h>
#include <wchar.h>
#include <stdlib.h>

int wmain(int argument_count, wchar_t **argument_values) {
    const wchar_t *arguments = GetCommandLineW();
    if (*arguments == L'"') {
        arguments++;
        while (*arguments && *arguments != L'"') arguments++;
        if (*arguments) arguments++;
    } else {
        while (*arguments && *arguments != L' ' && *arguments != L'\t') arguments++;
    }
    while (*arguments == L' ' || *arguments == L'\t') arguments++;
    const wchar_t *suffix = L" -C link-self-contained=yes";
    for (int index = 1; index < argument_count; index++) {
        if (wcscmp(argument_values[index], L"--target") == 0 ||
            wcsncmp(argument_values[index], L"--target=", 9) == 0) {
            suffix = L"";
            break;
        }
    }
    size_t length = wcslen(arguments) + wcslen(suffix) + 1;
    if (!*arguments || length > 32767) return 1;
    wchar_t *command = calloc(length, sizeof(wchar_t));
    if (!command) return 1;
    wcscpy(command, arguments);
    wcscat(command, suffix);
    STARTUPINFOW startup = { .cb = sizeof(startup) };
    PROCESS_INFORMATION process = {0};
    BOOL started = CreateProcessW(NULL, command, NULL, NULL, TRUE, 0, NULL, NULL, &startup, &process);
    free(command);
    if (!started) return 1;
    WaitForSingleObject(process.hProcess, INFINITE);
    DWORD status = 1;
    GetExitCodeProcess(process.hProcess, &status);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return (int)status;
}
