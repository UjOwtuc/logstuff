-- vim: set ft=sql nonu :

create sequence logs_id;

create table logs(
	id integer not null default nextval('logs_id'),
	tstamp timestamp with time zone not null,
	doc jsonb not null,
	search tsvector
) partition by range(tstamp);

create index idx_logs_id_tstamp on logs(id, tstamp);
create index idx_search on logs using GIN(search);

create table logs_2021_10 partition of logs for values from ('2021-10-01') to ('2021-11-01');

create or replace function try_to_int(text) returns integer as $$
begin
	return cast($1 as integer);
exception
	when invalid_text_representation then
		return null;
end;
$$ language plpgsql immutable;

