#include "stdafx.h"
#include "TimeTracking.h"

void TimeTracking::workerDelegate() {
	concurrency::task_group tasks;

	concurrency::parallel_invoke([&] {
		while (true) {
			for (const auto& process : processMap) {
				SYSTEMTIME sProcessTime;
				if (isProcessRunning(process.first + ".exe", sProcessTime)) {
					LOCK(processesMutex, processMap[process.first].first = true);

					auto lockCheck = [&] {
						bool returnVal;
						LOCK(processesMutex, returnVal = processMap[process.first].first && processMap[process.first].second);
						return returnVal;
					};

					if (lockCheck()) {
						tasks.run([&] {
							LOCK(processesMutex, processMap[process.first].second = false)
							
							auto start = chrono::high_resolution_clock::now();
							
							auto today = chrono::system_clock::now();
							time_t currentTime = chrono::system_clock::to_time_t(today);
							st_time *time = localtime(&currentTime);

							const st_time* appStartStruct;

							LOCK(startTimeMutex, appStartStruct = &restClient.getStartTime())

							int monthMulti = 2592000, dayMulti = 86400, hourMulti = 3600, minuteMulti = 60;

							auto appStartTime = (appStartStruct->tm_mon + 1) * monthMulti + (appStartStruct->tm_mday * dayMulti +
									appStartStruct->tm_hour * hourMulti + appStartStruct->tm_min * minuteMulti + appStartStruct->tm_sec);

							auto processStartTime = sProcessTime.wMonth * monthMulti + sProcessTime.wDay * dayMulti +
									((sProcessTime.wHour + 2) * hourMulti + sProcessTime.wMinute * minuteMulti + sProcessTime.wSecond);

							if (appStartTime < processStartTime) {
								LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::LAUNCHES, process.first))
							}

							auto lockCheck = [&] {
								bool returnVal;
								LOCK(processesMutex, returnVal = processMap[process.first].first);
								return returnVal;
							};

							while (lockCheck()) {
								chrono::duration<float> elapsed = chrono::high_resolution_clock::now() - start;
								if (int(elapsed.count()) != 0 && int(elapsed.count()) % 60 == 0) {
									LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::DURATION, process.first))
									LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::TIMELINE, process.first))
								}

								pplx::wait(1000);
							}

							chrono::duration<float> end = chrono::high_resolution_clock::now() - start;

							LOCK(onDataMutex, restClient.onDataRecieve(
								RestBase::RECIEVE_TYPES::LONGEST_SESSION, process.first, int(end.count() / 60)))
							LOCK(processesMutex, processMap[process.first].second = true);
						});
					}
				}
				else {
					LOCK(processesMutex, processMap[process.first].first = false);
				}
			}

			pplx::wait(5000);
		}
	},
		[&] { tasks.run([&] {
			while (true) {
				HWND hwnd = GetForegroundWindow();

				if (isFullscreen(hwnd)) {
					auto name = getWindowName(hwnd);

					if (name) {
						addProcess(*(name.get()));
					}
				}

				pplx::wait(22000);
			}
		});
	});
}

TimeTracking::TimeTracking(RestBase& restClient) : restClient(restClient), worker(thread([=] { workerDelegate(); })) {
	const vector<string>& processes = restClient.getProcesses();

	for (const string& process : processes) {
		processMap.emplace(process, make_pair(true, true));
	}
}

TimeTracking::~TimeTracking() {
	worker.join();
}

bool TimeTracking::addProcess(const string& process) {
	if (processMap.count(process) == 0) {
		if (restClient.putData(process)) {
			processMap.emplace(process, make_pair(true, true));
			return true;
		}
		else {
			return false;
		}
	}
	else {
		return false;
	}
}

bool TimeTracking::deleteProcess(const string& process) {
	if (processMap.count(process) > 0) {
		if (restClient.deleteData(process)) {
			processMap.emplace(process, make_pair(true, true));
			return true;
		}
		else {
			return false;
		}
	}
	else {
		return false;
	}
}

bool TimeTracking::isProcessRunning(const string& process, SYSTEMTIME& sProcessTime) {
	PROCESSENTRY32 pe;
	HANDLE handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
	pe.dwSize = sizeof(PROCESSENTRY32);
	HANDLE processHandle;
	FILETIME fProcessTime, ftExit, ftKernel, ftUser;
	if (!Process32First(handle, &pe)) {
		CloseHandle(handle);
		return false;
	}

	do {
		if (!lstrcmpi(pe.szExeFile, conversions::to_string_t(process).c_str())) {
			processHandle = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pe.th32ProcessID);
			GetProcessTimes(processHandle, &fProcessTime, &ftExit, &ftKernel, &ftUser);
			FileTimeToSystemTime(&fProcessTime, &sProcessTime);
			CloseHandle(processHandle);
			CloseHandle(handle);
			return true;
		}

	} while (Process32Next(handle, &pe));

	CloseHandle(handle);

	return false;
}

bool TimeTracking::isFullscreen(HWND hwnd)
{
	RECT a, b;
	GetWindowRect(hwnd, &a);
	GetWindowRect(GetDesktopWindow(), &b);

	return a.left == b.left  && a.top == b.top   &&
			a.right == b.right && a.bottom == b.bottom;
}

unique_ptr<string> TimeTracking::getWindowName(HWND hwnd) {
	WCHAR windowTitle[256];
	GetWindowText(hwnd, windowTitle, 256);

	DWORD pid;
	GetWindowThreadProcessId(hwnd, &pid);

	HANDLE handle = OpenProcess(
		PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);

	if (handle) {
		WCHAR buffer[MAX_PATH];
		if (GetProcessImageFileName(handle, buffer, MAX_PATH)) {
			CloseHandle(handle);
			auto path = conversions::utf16_to_utf8(buffer);

			auto base = string(
				std::find_if(path.rbegin(), path.rend(),
					[](char ch) { return ch == '\\' || ch == '/'; }).base(), path.end());

			auto pivot = std::find(base.rbegin(), base.rend(), '.');

			auto baseNameWithoutExtension = pivot == base.rend()
				? base : std::string(base.begin(), pivot.base() - 1);

			return make_unique<string>(baseNameWithoutExtension);
		}
		else {
			CloseHandle(handle);
			return nullptr;
		}
	}

	return nullptr;
}