#include "stdafx.h"
#include "TimeTracking.h"

void TimeTracking::workerDelegate() {
	concurrency::task_group tasks;
	concurrency::parallel_invoke([&] {
		while (true) {
			for (const auto& process : processMap) {
				if (isProcessRunning(process.first + ".exe")) {
					LOCK(processesMutex, processMap[process.first].first = true);

					auto lockCheck = [&] {
						bool returnVal;
						LOCK(processesMutex, returnVal = processMap[process.first].first && processMap[process.first].second);
						return returnVal;
					};

					if (lockCheck()) {
						tasks.run([&] {
							LOCK(processesMutex, processMap[process.first].second = false);

							int launches = restClient.getData("apps/" + process.first + "/launches").as_integer();
							restClient.patchData(process.first, "launches", json::value::number(++launches));

							auto start = std::chrono::high_resolution_clock::now();
							
							auto today = std::chrono::system_clock::now();
							std::time_t currentTime = std::chrono::system_clock::to_time_t(today);
							struct tm *time = std::localtime(&currentTime);

							auto lockCheck = [&] {
								bool returnVal;
								LOCK(processesMutex, returnVal = processMap[process.first].first);
								return returnVal;
							};

							auto todayDuration = restClient.getData("apps/" + process.first + "/timeline/year/"
								+ to_string(time->tm_year + 1900) + "/month/" + to_string(time->tm_mon)
								+ "/day/" + to_string(time->tm_mday)).as_integer();

							auto duration = restClient.getData("apps/" + process.first + "/duration").as_integer();

							while (lockCheck()) {
								std::chrono::duration<float> elapsed = std::chrono::high_resolution_clock::now() - start;
								if (int(elapsed.count()) != 0 && int(elapsed.count()) % 60 == 0) {
									restClient.patchData(process.first, "duration", json::value::number(++duration));

									restClient.patchData(process.first, "timeline/year/"
										+ to_string(time->tm_year + 1900) + "/month/" + to_string(time->tm_mon)
										+ "/day/" + to_string(time->tm_mday), json::value::number(++todayDuration));
								}

								pplx::wait(1000);
							}

							std::chrono::duration<float> end = std::chrono::high_resolution_clock::now() - start;

							restClient.patchData(process.first, "longestSession", json::value::number(
								max(int(end.count() / 60), restClient.getData("apps/" + process.first + "/longestSession").as_integer())));

							LOCK(processesMutex, processMap[process.first].second = true);
						});
					}
				}
				else {
					LOCK(processesMutex, processMap[process.first].first = false);
				}
			}

			pplx::wait(10000);
		}
	},
		[&] { tasks.run([] {});
	});
}

TimeTracking::TimeTracking(RestBase& restClient) : restClient(restClient), worker(thread([=] { workerDelegate(); })) {
	auto processNames = restClient.getData("apps");

	for (auto iter = processNames.as_object().cbegin(); iter != processNames.as_object().cend(); ++iter) {
		processMap.emplace(conversions::to_utf8string(iter->first), make_pair(true, true));
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

bool TimeTracking::isProcessRunning(const string& process) {
	PROCESSENTRY32 pe;
	HANDLE handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
	pe.dwSize = sizeof(PROCESSENTRY32);

	if (!Process32First(handle, &pe)) {
		CloseHandle(handle);
		return false;
	}

	do {
		if (!lstrcmpi(pe.szExeFile, conversions::to_string_t(process).c_str())) {
			CloseHandle(handle);
			return true;
		}

	} while (Process32Next(handle, &pe));

	CloseHandle(handle);

	return false;
}