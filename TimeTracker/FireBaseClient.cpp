#include "stdafx.h"
#include "FireBaseClient.h"

FirebaseClient::FirebaseClient(const string& baseUrl) : RestBase(baseUrl) { }

FirebaseClient::FirebaseClient(const string& baseUrl, const string& authentication) : RestBase(baseUrl, authentication) { }

json::value FirebaseClient::getData(const string& searchQuery) {
	uri_builder builder(conversions::to_string_t(searchQuery + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return getDataBase(builder);
}

bool FirebaseClient::deleteData(const string& appName)
{
	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return deleteDataBase(builder);
}

bool FirebaseClient::putData(const string& appName)
{
	auto now = std::chrono::system_clock::now();
	std::time_t currentTime = std::chrono::system_clock::to_time_t(now);
	struct tm *time = std::localtime(&currentTime);

	json::value content;
	content[L"duration"] = json::value::number(0);
	content[L"name"] = json::value::string(conversions::to_string_t(appName));
	content[L"launches"] = json::value::number(0);
	content[L"longestSession"] = json::value::number(0);

	json::value day;
	day[L"day"] = json::value::array();
	day[L"day"][time->tm_mday] = json::value::number(0);

	json::value month;
	month[L"month"] = json::value::array();
	month[L"month"][time->tm_mon] = day;

	json::value year;
	year[L"year"] = json::value::array();
	year[L"year"][time->tm_year + 1900] = month;

	content[L"timeline"] = year;

	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return putDataBase(builder, content);
}

bool FirebaseClient::patchData(const string& appName, const string& key, const json::value& value)
{
	json::value content;
	content[conversions::to_string_t(key)] = value;

	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return patchDataBase(builder, content);
}
