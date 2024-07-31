CREATE TABLE `hosts`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`name` TEXT NOT NULL,
	`username` TEXT NOT NULL,
	`hostname` TEXT NOT NULL,
	`port` SMALLINT NOT NULL
);

CREATE TABLE `users`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`username` TEXT UNIQUE NOT NULL,
	`enabled`  BOOLEAN NOT NULL CHECK (enabled IN (0, 1)) DEFAULT 1
);

CREATE TABLE `user_in_host`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`host_id` INTEGER NOT NULL,
	`user_id` INTEGER NOT NULL,
	`options` TEXT,
	FOREIGN KEY (`host_id`) REFERENCES `hosts`(`id`),
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`)
);

CREATE TABLE `groups`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`name` TEXT NOT NULL
);

CREATE TABLE `user_keys`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`key_type` TEXT NOT NULL,
	`key_base64` TEXT UNIQUE NOT NULL,
	`comment` TEXT,
	`user_id` INTEGER NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`)
);

CREATE TABLE `host_keys`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`key_type` TEXT NOT NULL,
	`key_base64` TEXT NOT NULL,
	`comment` TEXT,
	`host_id` INTEGER NOT NULL,
	FOREIGN KEY (`host_id`) REFERENCES `hosts`(`id`)
);
