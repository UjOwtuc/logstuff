#ifndef TIMERANGEMODEL_H
#define TIMERANGEMODEL_H

#include <QAbstractListModel>
#include <QStyledItemDelegate>

class TimeSpec;

class TimerangeModel : public QAbstractListModel
{
public:
	explicit TimerangeModel(QObject* parent = nullptr);

	int rowCount(const QModelIndex & parent) const;
	QVariant data(const QModelIndex & index, int role) const;
	QVariant headerData(int section, Qt::Orientation orientation, int role) const;

	int addChoice(const TimeSpec& start, const TimeSpec& end);

private:
	QList<QPair<TimeSpec, TimeSpec>> m_data;
};

#endif // TIMERANGEMODEL_H
