#include "stdafx.h"
#include "FireBaseClient.h"
#include "TimeTracking.h"

#include <iostream>

int main()
{
	FirebaseClient fireBaseClient(RestBase::readConfig("baseUri"), RestBase::readConfig("apiKey"));

	TimeTracking tracking(fireBaseClient);

	tracking.addProcess("notepad");
	tracking.addProcess("sublime_text");
	tracking.addProcess("calc");

    return 0;
}

